use tsuzuri::syntax::Edition;
use tsuzuri::{analyze, analyze_modules, analyze_with_edition, llvm, parser};

fn ir(module: &tsuzuri::check::CheckedModule, wasm: bool) -> String {
    let first = llvm::emit_target(module, llvm::Entry::Library, wasm).unwrap();
    assert_eq!(
        first,
        llvm::emit_target(module, llvm::Entry::Library, wasm).unwrap(),
        "IR is deterministic"
    );
    first
}

fn code(source: &str) -> &'static str {
    match analyze(source) {
        Ok(_) => "ok",
        Err(error) => error.code,
    }
}

#[test]
fn parses_link_names() {
    let source = "extern \"sqrt\" def c_sqrt :: f64 -> f64\nextern \"env\" \"host_now\" def host_now :: unit -> i64\nprivate extern \"x\" def hidden :: i64\nextern def plain :: i64\n42";
    let program = parser::parse(source).unwrap();
    assert_eq!(program.externs.len(), 4);
    let link = program.externs[0].link.as_ref().unwrap();
    assert_eq!(
        (link.symbol.as_str(), link.module.is_none()),
        ("sqrt", true)
    );
    let link = program.externs[1].link.as_ref().unwrap();
    assert_eq!(link.symbol, "host_now");
    assert_eq!(
        link.module.as_ref().map(|(name, _)| name.as_str()),
        Some("env")
    );
    assert!(program.externs[2].link.is_some() && program.externs[3].link.is_none());
    let formatted =
        tsuzuri::formatter::format_source("Main.tz", source, tsuzuri::syntax::SourceKind::Code)
            .unwrap();
    assert!(
        formatted
            .formatted
            .contains("extern \"env\" \"host_now\" def host_now")
    );
    analyze(&formatted.formatted).unwrap();
    for (source, expected) in [
        ("extern \"a\" \"b\" \"c\" def host :: i64", "E0002"),
        ("extern \"x\" type Host", "E0002"),
        ("extern \"x\" export def host :: i64", "E1008"),
        ("extern \"\\uD800\" def host :: i64", "E0002"),
        ("extern \"x\" fn host() -> i64 { 1 }", "E0002"),
    ] {
        assert_eq!(
            parser::parse(source).unwrap_err().code,
            expected,
            "{source}"
        );
    }
}

#[test]
fn link_names_lower_to_symbols_and_wasm_modules() {
    let source = "extern \"sqrt\" def c_sqrt :: f64 -> f64\nextern \"env\" \"host_now\" def host_now :: unit -> i64\nextern def unused :: i64\nfn root(value: f64) -> i64 { if c_sqrt value == 1.5 then host_now () else 0 }";
    let module = analyze(source).unwrap();
    let native = ir(&module, false);
    assert!(native.contains("declare double @sqrt(double)\n"));
    assert!(native.contains("declare i64 @host_now()\n"));
    assert!(!native.contains("wasm-import"));
    let wasm = ir(&module, true);
    assert!(wasm.contains(
        "declare double @sqrt(double) \"wasm-import-module\"=\"tsuzuri\" \"wasm-import-name\"=\"sqrt\""
    ));
    assert!(wasm.contains(
        "declare i64 @host_now() \"wasm-import-module\"=\"env\" \"wasm-import-name\"=\"host_now\""
    ));
    // An explicit symbol is the host's own: the header gets no prototype for it.
    let header = llvm::header(&module);
    assert!(!header.contains("sqrt(") && !header.contains("host_now("));
    // The same symbol declared in two modules is one declaration.
    let module = analyze_modules(&[
        (
            "Roots.tz",
            "extern \"sqrt\" def c_sqrt :: f64 -> f64\nfn root(value: f64) -> f64 { c_sqrt value }",
        ),
        (
            "Main.tz",
            "extern \"sqrt\" def sqrt_again :: f64 -> f64\nfn twice(value: f64) -> f64 { sqrt_again value }",
        ),
    ])
    .unwrap();
    for wasm in [false, true] {
        assert_eq!(
            ir(&module, wasm)
                .matches("declare double @sqrt(double)")
                .count(),
            1
        );
    }
    // Externs without a link name keep their names.
    let module = analyze("extern def now :: unit -> i64\nfn root() -> i64 { now () }").unwrap();
    assert!(ir(&module, false).contains("declare i64 @tsuzuri_host_Main_now()"));
    assert!(
        ir(&module, true)
            .contains("\"wasm-import-module\"=\"tsuzuri\" \"wasm-import-name\"=\"Main.now\"")
    );
}

#[test]
fn same_symbol_with_one_c_abi_is_one_declare() {
    let source = "extern type Hwnd
record Rect { left: i32, top: i32, right: i32, bottom: i32 }
extern \"SendMessageW\" def send_message :: ref Hwnd -> i32u -> i64u -> i64 -> i64
extern \"SendMessageW\" def send_rect :: ref Hwnd -> i32u -> i64u -> ref Rect -> i64
def on_event :: ref Hwnd -> i32u -> i64u -> i64 -> i64 = \\_hwnd _msg _wparam lparam -> lparam
extern \"SetWindowLongPtrW\" def set_window_long :: ref Hwnd -> i32 -> i64 -> i64
extern \"SetWindowLongPtrW\" def set_window_proc :: ref Hwnd -> i32 -> (ref Hwnd -> i32u -> i64u -> i64 -> i64) -> i64
export def run :: ref Hwnd -> i64
fn run hwnd =
    let r = Rect { left: 1, top: 2, right: 3, bottom: 4 }
    send_message hwnd 1i32u 2i64u 3 +
    send_rect hwnd 1i32u 2i64u (ref r) +
    set_window_long hwnd (-4i32) 0 +
    set_window_proc hwnd (-4i32) on_event
";
    let module = analyze(source).unwrap();
    let emitted = ir(&module, false);
    assert_eq!(
        emitted
            .matches("declare i64 @SendMessageW(ptr, i32, i64, i64)\n")
            .count(),
        1,
        "{emitted}"
    );
    assert_eq!(
        emitted
            .matches("declare i64 @SetWindowLongPtrW(ptr, i32, i64)\n")
            .count(),
        1,
        "{emitted}"
    );
    assert!(emitted.contains("ptrtoint ptr "), "{emitted}");
    assert!(
        emitted.contains("ptrtoint ptr @tz.callback.Main.on_event to i64"),
        "{emitted}"
    );
}

#[test]
fn canonical_declare_ignores_order_and_same_types_keep_ir() {
    let forward = "extern type H
extern \"ffi_pick\" def pick_int :: i64 -> i64
extern \"ffi_pick\" def pick_handle :: H -> H
export def run :: H -> H
fn run h =
    let _ = pick_int 1
    pick_handle h
";
    let reverse = "extern type H
extern \"ffi_pick\" def pick_handle :: H -> H
extern \"ffi_pick\" def pick_int :: i64 -> i64
export def run :: H -> H
fn run h =
    let _ = pick_int 1
    pick_handle h
";
    let f_mod = analyze(forward).unwrap();
    let r_mod = analyze(reverse).unwrap();
    let f_ir = ir(&f_mod, false);
    let r_ir = ir(&r_mod, false);
    for text in [&f_ir, &r_ir] {
        assert_eq!(
            text.matches("declare i64 @ffi_pick(i64)\n").count(),
            1,
            "{text}"
        );
        assert!(text.contains("inttoptr i64 "), "{text}");
    }

    let one = analyze(
        "extern \"sqrt\" def a :: f64 -> f64\nexport def run :: f64 -> f64\nfn run x = a x\n",
    )
    .unwrap();
    let two = analyze("extern \"sqrt\" def a :: f64 -> f64\nextern \"sqrt\" def b :: f64 -> f64\nexport def run :: f64 -> f64\nfn run x = a x\n").unwrap();
    assert!(two.extern_abis.is_empty());
    assert_eq!(ir(&one, false), ir(&two, false));
}

#[test]
fn class_mismatches_and_same_class_variants_follow_c_abi() {
    use tsuzuri::diagnostic::RelatedRole;
    for (source, needle) in [
        (
            "extern \"f\" def a :: i64 -> i64\nextern \"f\" def b :: i32 -> i64\n1",
            "parameter 1 is a 32-bit integer, but a 64-bit integer or pointer in the first declaration",
        ),
        (
            "extern \"f\" def a :: i64 -> i64\nextern \"f\" def b :: f64 -> i64\n1",
            "parameter 1 is a 64-bit float, but a 64-bit integer or pointer in the first declaration",
        ),
        (
            "extern \"f\" def a :: i32 -> i64\nextern \"f\" def b :: f32 -> i64\n1",
            "parameter 1 is a 32-bit float, but a 32-bit integer in the first declaration",
        ),
        (
            "extern \"f\" def a :: ref string -> i64\nextern \"f\" def b :: i64 -> i64\n1",
            "takes 1 C parameter here but 2 in the first declaration",
        ),
        (
            "extern \"f\" def a :: i64 -> i64\nextern \"f\" def b :: i64 -> unit\n1",
            "the result is void, but a 64-bit integer or pointer in the first declaration",
        ),
    ] {
        let error = analyze(source).unwrap_err();
        assert_eq!(
            (error.code, error.kind),
            ("E1008", Some("conflicting_extern_declaration")),
            "{source}"
        );
        assert!(
            error.message.contains(needle),
            "{source}: {}",
            error.message
        );
        assert!(
            error
                .related
                .iter()
                .any(|item| item.role == RelatedRole::FirstDefinedHere),
            "{source}: {:?}",
            error.related
        );
    }

    // Module mismatch keeps the original message without related notes.
    let mod_err = analyze(
        "extern \"a\" \"f\" def x :: i64 -> i64\nextern \"b\" \"f\" def y :: i64 -> i64\n1",
    )
    .unwrap_err();
    assert_eq!(
        mod_err.message,
        "extern symbol 'f' is declared with different signatures or import modules; declare it once and call it from other modules"
    );
    assert!(mod_err.related.is_empty());

    // Same-class declarations pass.
    for ok in [
        "extern \"f\" def a :: i64 -> bool\nextern \"f\" def b :: i64 -> i32\n1",
        "extern \"f\" def a :: i8u -> i64\nextern \"f\" def b :: i32 -> i64\n1",
        "extern \"f\" def a :: (i64 -> i64) -> i64\nextern \"f\" def b :: i64 -> i64\n1",
        "record R { x: i64 }\nextern \"f\" def a :: unit -> R\nextern \"f\" def b :: i64 -> unit\n1",
    ] {
        assert_eq!(code(ok), "ok", "{ok}");
    }
}

#[test]
fn wasm_rejects_differently_typed_reached_declarations() {
    let both_reached = "extern type H
extern \"ffi_word\" def word_int :: i64 -> i64
extern \"ffi_word\" def word_handle :: ref H -> i64
export def run :: ref H -> i64
fn run h = word_int 1 + word_handle h
";
    let module = analyze(both_reached).unwrap();
    assert!(ir(&module, false).contains("declare i64 @ffi_word(i64)"));
    let wasm_err = llvm::emit_target(&module, llvm::Entry::Library, true).unwrap_err();
    assert_eq!(
        (wasm_err.code, wasm_err.kind),
        ("E1008", Some("conflicting_extern_declaration"))
    );
    assert!(
        wasm_err
            .message
            .contains("WebAssembly imports one signature per name"),
        "{}",
        wasm_err.message
    );

    // Reaching only one of the declarations succeeds on WASM.
    let one_reached = "extern type H
extern \"ffi_word\" def word_int :: i64 -> i64
extern \"ffi_word\" def word_handle :: ref H -> i64
export def run :: i64 -> i64
fn run x = word_int x
";
    let module = analyze(one_reached).unwrap();
    assert!(ir(&module, true).contains("declare i64 @ffi_word(i64)"));
}

#[test]
fn link_names_reject_reserved_invalid_and_conflicting() {
    let long = "a".repeat(256);
    let allowed = "a".repeat(255);
    for source in [
        "extern \"tz_add\" def bad :: i64 -> i64".to_owned(),
        "extern \"tsuzuri_x\" def bad :: i64 -> i64".to_owned(),
        "extern \"tsuzuri\" def bad :: i64 -> i64".to_owned(),
        "extern \"__x\" def bad :: i64 -> i64".to_owned(),
        "extern \"malloc\" def bad :: i64 -> i64".to_owned(),
        "extern \"write\" def bad :: i64 -> i64".to_owned(),
        // Helpers of the embedded numeric runtime would be redefined once it is linked in.
        "extern \"decode\" def bad :: i64 -> i64".to_owned(),
        "extern \"2x\" def bad :: i64 -> i64".to_owned(),
        "extern \"a-b\" def bad :: i64 -> i64".to_owned(),
        "extern \"\" def bad :: i64 -> i64".to_owned(),
        format!("extern \"{long}\" def bad :: i64 -> i64"),
        "extern \"tsuzuri_io\" \"f\" def bad :: i64".to_owned(),
        "extern \"a b\" \"f\" def bad :: i64".to_owned(),
        "extern \"\" \"f\" def bad :: i64".to_owned(),
        "extern \"f\" def a :: i64 -> i64\nextern \"f\" def b :: f64 -> f64".to_owned(),
        "extern \"m1\" \"f\" def a :: i64\nextern \"m2\" \"f\" def b :: i64".to_owned(),
        "extern \"f\" def a :: i64\nextern \"env\" \"f\" def b :: i64".to_owned(),
    ] {
        assert_eq!(code(&source), "E1008", "{source}");
    }
    for source in [
        format!("extern \"{allowed}\" def ok :: i64"),
        "extern \"tsuzuri\" \"f\" def ok :: i64".to_owned(),
        "extern \"a-b.c_d\" \"f\" def ok :: i64".to_owned(),
        "extern \"f\" def a :: i64 -> i64\nextern \"f\" def b :: i64 -> i64".to_owned(),
        "extern \"env\" \"f\" def a :: i64\nextern \"env\" \"f\" def b :: i64".to_owned(),
    ] {
        assert_eq!(code(&source), "ok", "{source}");
    }
    // A symbol keeps its type and module across modules too.
    let error = analyze_modules(&[
        ("Other.tz", "extern \"f\" def a :: i64 -> i64"),
        ("Main.tz", "extern \"f\" def b :: f64 -> f64\n1"),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1008");
    // The existing naming errors are unchanged.
    assert_eq!(
        code("extern def now :: i64\nextern def now :: i64"),
        "E1001"
    );
}

const HANDLES: &str = "extern type Counter
extern \"e12_counter_new\" def counter_new :: i64 -> Counter
extern \"e12_counter_peek\" def counter_peek :: ref Counter -> i64 -> i64
extern \"e12_counter_free\" def counter_free :: Counter -> i64
";

fn with_handles(body: &str) -> String {
    format!("{HANDLES}\n{body}\n")
}

#[test]
fn parses_copy_extern_types() {
    use tsuzuri::syntax::{HandleKind, Visibility};
    assert_eq!(
        std::mem::size_of::<tsuzuri::check::Type>(),
        4 * std::mem::size_of::<usize>(),
        "Type must stay four words (W18 S1)"
    );
    let program = parser::parse(
        "extern type copy A\nprivate extern type copy B\n/// documented\nextern type copy C\nextern type copy\nextern type copy local\n42",
    )
    .unwrap();
    let summary: Vec<_> = program
        .extern_types
        .iter()
        .map(|handle| (handle.name.text.as_str(), handle.kind, handle.visibility))
        .collect();
    assert_eq!(
        summary,
        [
            ("A", HandleKind::Copy, Visibility::Public),
            ("B", HandleKind::Copy, Visibility::Private),
            ("C", HandleKind::Copy, Visibility::Public),
            ("copy", HandleKind::Owned, Visibility::Public),
            ("local", HandleKind::Copy, Visibility::Public),
        ]
    );
    assert!(program.extern_types[2].doc.is_some());
    assert!(program.extern_types[0].kind_span.is_some());
    assert!(program.extern_types[3].kind_span.is_none());

    let conflict = parser::parse("extern type copy copy X\n42").unwrap_err();
    assert_eq!(
        (conflict.code, conflict.kind, conflict.message.as_str()),
        (
            "E0002",
            Some("extern_type_modifier_conflict"),
            "an extern type takes at most one of 'copy' and 'local'; remove one",
        )
    );

    let formatted = tsuzuri::formatter::format_source(
        "Main.tz",
        "extern   type  copy   A\nprivate   extern  type   copy  B\nextern   type   copy\n42",
        tsuzuri::syntax::SourceKind::Code,
    )
    .unwrap();
    assert_eq!(
        formatted.formatted,
        "extern type copy A\nprivate extern type copy B\nextern type copy\n42\n"
    );
}

#[test]
fn copy_extern_types_are_copy_capturable_send_sync() {
    let prelude = "extern type copy Hwnd
extern \"w18_hwnd\" def make_hwnd :: i64 -> Hwnd
extern \"w18_use\" def use_hwnd :: Hwnd -> i64
extern \"w18_peek\" def peek_hwnd :: ref Hwnd -> i64
";
    for body in [
        "let h = make_hwnd 1\nlet copied = h\nuse_hwnd h + use_hwnd copied + peek_hwnd (ref h)",
        "let h = make_hwnd 1\nlet tag = \"win\"\nlet f = \\x -> use_hwnd h + tag.length + x\nlet g = f\nf 1 + g 2",
        "record WindowState { hwnd: Hwnd, id: i64 }\nlet s = WindowState { hwnd: make_hwnd 1, id: 7 }\nlet s2 = s\nuse_hwnd s.hwnd + use_hwnd s2.hwnd",
        "union Tree = Leaf of Hwnd | Node of Tree * Tree\nlet t = Node (Leaf (make_hwnd 1), Leaf (make_hwnd 2))\nlet f = \\() -> match t with | Leaf h -> use_hwnd h | Node _ -> 3\nlet g = f\nf () + g ()",
        "let h = make_hwnd 1\nmatch DataChannel.bounded 1 with\n| (sender, receiver) ->\n    tzroutine {\n        let _ = DataChannel.send (ref sender) (use_hwnd h)\n        ()\n    }\n    match DataChannel.recv (ref receiver) with | Some v -> v + use_hwnd h | None -> 0",
        "let h = make_hwnd 1\nlet counts = Routine.scope (ref h) 2 (\\w _i -> peek_hwnd w)\ncounts[0] + counts[1]",
        "let shared = Arc.new (make_hwnd 1)\nlet copy_arc = Arc.share (ref shared)\nmatch DataChannel.bounded 1 with\n| (sender, receiver) ->\n    tzroutine {\n        let _ = DataChannel.send (ref sender) (peek_hwnd (Arc.get (ref copy_arc)))\n        ()\n    }\n    match DataChannel.recv (ref receiver) with | Some v -> v + peek_hwnd (Arc.get (ref shared)) | None -> 0",
    ] {
        for edition in [Edition::E2026, Edition::E2027] {
            let source = format!("{prelude}\n{body}\n");
            let module = analyze_with_edition(&source, edition)
                .unwrap_or_else(|e| panic!("{edition:?} rejected:\n{source}\n{e:?}"));
            let _ = ir(&module, false);
        }
    }

    // Plain-data, const, instance, and ABI position restrictions remain unchanged.
    for (bad, expected) in [
        ("const BAD: Hwnd = make_hwnd 1\n1", "E1026"),
        (
            "record Boxed { h: Hwnd }\nexport def out :: Boxed -> i64\nfn out b = 1\n1",
            "E1008",
        ),
        (
            "extern \"w18_mut\" def mut_hwnd :: ref mut Hwnd -> i64\n1",
            "ok",
        ),
        (
            "extern type OwnedH\nextern \"w18_mut\" def mut_owned :: ref mut OwnedH -> i64\n1",
            "E1008",
        ),
        (
            "export def mut_exp :: ref mut Hwnd -> i64\nfn mut_exp _h = 1\n1",
            "E1008",
        ),
        ("make_hwnd 1 == make_hwnd 2", "E1005"),
        ("to_string (make_hwnd 1)", "E1005"),
        ("pure def bad :: own Hwnd -> i64\nfn bad _h = 1\n1", "E1029"),
    ] {
        assert_eq!(code(&format!("{prelude}\n{bad}")), expected, "{bad}");
    }
}

#[test]
fn copy_handles_lower_like_handles() {
    let body = "extern \"w18_new\" def make_h :: i64 -> H
extern \"w18_peek\" def peek_h :: ref H -> i64
extern \"w18_take\" def take_h :: H -> i64
export def run :: own i64 -> i64
fn run x =
    let h = make_h x
    let seen = peek_h (ref h)
    seen + take_h h
export def forward :: own H -> H
fn forward h = h
";
    let owned = analyze(&format!("extern type H\n{body}")).unwrap();
    let copy = analyze(&format!("extern type copy H\n{body}")).unwrap();
    for wasm in [false, true] {
        assert_eq!(ir(&owned, wasm), ir(&copy, wasm), "wasm={wasm}");
    }
    assert_eq!(llvm::header(&owned), llvm::header(&copy));
}

#[test]
fn parses_extern_types() {
    let program =
        parser::parse("extern type A\nprivate extern type B\n/// documented\nextern type C\n42")
            .unwrap();
    let names: Vec<_> = program
        .extern_types
        .iter()
        .map(|handle| handle.name.text.as_str())
        .collect();
    assert_eq!(names, ["A", "B", "C"]);
    assert_eq!(
        program.extern_types[0].visibility,
        tsuzuri::syntax::Visibility::Public
    );
    assert_eq!(
        program.extern_types[1].visibility,
        tsuzuri::syntax::Visibility::Private
    );
    assert!(program.extern_types[2].doc.is_some());
    let formatted = tsuzuri::formatter::format_source(
        "Main.tz",
        "extern   type  A\nprivate extern type B\n42",
        tsuzuri::syntax::SourceKind::Code,
    )
    .unwrap();
    assert_eq!(
        formatted.formatted,
        "extern type A\nprivate extern type B\n42\n"
    );
    for (source, code, message) in [
        (
            "extern type T<A>",
            "E0002",
            "extern type declarations have no type parameters or definition",
        ),
        (
            "extern type T = i64",
            "E0002",
            "extern type declarations have no type parameters or definition",
        ),
        (
            "extern \"x\" type T",
            "E0002",
            "extern type declarations take no link name; remove the string",
        ),
        (
            "extern \"a\" \"b\" type T",
            "E0002",
            "extern type declarations take no link name; remove the string",
        ),
        (
            "export extern type T",
            "E1008",
            "extern declarations cannot be exported",
        ),
        (
            "extern export type T",
            "E1008",
            "extern declarations cannot be exported",
        ),
    ] {
        let error = parser::parse(source).unwrap_err();
        assert_eq!(
            (error.code, error.message.as_str()),
            (code, message),
            "{source}"
        );
    }
    assert_eq!(parser::parse("extern type").unwrap_err().code, "E0002");
}

#[test]
fn extern_types_are_noncopy_leaf_types() {
    // Moving a handle ends its use; borrowing does not.
    assert_eq!(
        code(&with_handles(
            "def twice :: i64 -> i64\nfn twice start =\n    let counter = counter_new start\n    let first = counter_free counter\n    first + counter_free counter\ntwice 1"
        )),
        "E1012"
    );
    assert_eq!(
        code(&with_handles(
            "def peeks :: i64 -> i64\nfn peeks start =\n    let counter = counter_new start\n    let first = counter_peek (ref counter) 1\n    first + counter_peek (ref counter) 2 + counter_free counter\npeeks 1"
        )),
        "ok"
    );
    // A function value would copy the handle with its environment.
    assert_eq!(
        code(&with_handles(
            "def capture :: i64 -> i64\nfn capture start =\n    let counter = counter_new start\n    let f = \\x -> counter_peek (ref counter) x\n    f 1\ncapture 1"
        )),
        "E1005"
    );
    // The handle has no instances: it is neither displayed nor compared.
    for body in [
        "def show :: i64 -> string\nfn show start = to_string (counter_new start)\nshow 1",
        "def same :: i64 -> bool\nfn same start = counter_new start == counter_new start\nsame 1",
    ] {
        assert_eq!(code(&with_handles(body)), "E1005", "{body}");
    }
    assert_eq!(
        code("record Counter { x: i64 }\nextern type Counter\n1"),
        "E1001"
    );
    assert_eq!(code("extern type Counter\nextern type Counter\n1"), "E1001");
    assert_eq!(
        code("extern type Counter\nunion Counter = A | B\n1"),
        "E1001"
    );
    assert_eq!(code("extern type i64\n1"), "E1001");
    // Outside the C ABI a handle is an ordinary owned value.
    for body in [
        "def store :: i64 -> i64\nfn store start =\n    let handles = [counter_new start, counter_new 2]\n    handles.length\nstore 1",
        "def keep :: i64 -> i64\nfn keep start =\n    let held: Maybe<Counter> = Some (counter_new start)\n    match held with\n    | Some counter -> counter_free counter\n    | None -> 0\nkeep 1",
        "record Holder { counter: Counter }\ndef hold :: i64 -> Holder\nfn hold start = Holder { counter: counter_new start }\n1",
        "def id :: 'a -> 'a\nfn id x = x\ndef through :: i64 -> i64\nfn through start = counter_free (id (counter_new start))\nthrough 1",
    ] {
        assert_eq!(code(&with_handles(body)), "ok", "{body}");
    }
    // An extern type takes no type arguments and is not a type constructor.
    assert_eq!(
        code(&with_handles(
            "def bad :: Counter<i64> -> i64\nfn bad c = 1\n1"
        )),
        "E1004"
    );
}

#[test]
fn extern_types_resolve_across_modules() {
    let module = analyze_modules(&[
        (
            "Hosts.tz",
            "extern type Counter\nextern \"e12_counter_new\" def counter_new :: i64 -> Counter",
        ),
        (
            "Main.tz",
            "def make :: i64 -> Hosts.Counter\nfn make start = Hosts.counter_new start\ndef plain :: i64 -> Counter\nfn plain start = Hosts.counter_new start\n1",
        ),
    ])
    .unwrap();
    assert!(ir(&module, false).contains("declare ptr @e12_counter_new(i64)"));
    let error = analyze_modules(&[
        ("Hosts.tz", "private extern type Secret"),
        ("Main.tz", "def f :: Secret -> i64\nfn f secret = 1\n1"),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1022");
    // A public function cannot expose a private extern type.
    assert_eq!(
        code("private extern type Secret\nextern \"e12_secret\" def secret :: i64 -> Secret\n1"),
        "E1022"
    );
    // Two modules may each declare a `Counter`; the unqualified name is then ambiguous.
    let error = analyze_modules(&[
        ("One.tz", "extern type Counter"),
        ("Two.tz", "extern type Counter"),
        ("Main.tz", "def f :: Counter -> i64\nfn f c = 1\n1"),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1004");
}

#[test]
fn handles_lower_to_pointers_and_header_typedefs() {
    let source = with_handles(
        "export def counters :: i64 -> i64\nfn counters start =\n    let counter = counter_new start\n    let first = counter_peek (ref counter) 5\n    first + counter_free counter\nexport def pass_through :: Counter -> Counter\nfn pass_through counter = counter\nexport def borrowed :: ref Counter -> i64\nfn borrowed counter = counter_peek counter 3",
    );
    let module = analyze(&source).unwrap();
    for wasm in [false, true] {
        let ir = ir(&module, wasm);
        assert!(ir.contains("declare ptr @e12_counter_new(i64)"));
        assert!(ir.contains("declare i64 @e12_counter_peek(ptr, i64)"));
        assert!(ir.contains("declare i64 @e12_counter_free(ptr)"));
        assert!(ir.contains("define ptr @tz_pass_through(ptr %arg0)"));
        assert!(ir.contains("define i64 @tz_borrowed(ptr %arg0)"));
        // A borrowed handle is read from its slot before the host sees it.
        assert!(ir.contains("load ptr, ptr"));
        // The host passes the handle itself, so the export spills it and lends the slot (E13);
        // on native, NS01 checks that %arg0 is non-null before spilling.
        if wasm {
            assert!(ir.contains(
                "  %h0 = alloca ptr, align 8\n  store ptr %arg0, ptr %h0\n  %result = call i64 @tz.fn.Main.borrowed(ptr %h0)"
            ));
        } else {
            assert!(
                ir.contains("icmp ne ptr %arg0, null")
                    && ir.contains("call i64 @tz.fn.Main.borrowed(ptr "),
                "{ir}"
            );
        }
        // Handles are plain pointers: no buffer ABI use and no allocator export.
        assert_eq!(
            ir.matches("%tz.abi.buffer").count(),
            1,
            "type definition only"
        );
        assert!(!ir.contains("@tsuzuri_alloc"));
    }
    let header = llvm::header(&module);
    let typedef = "typedef struct tz_handle_4Main_7Counter_s *tz_handle_4Main_7Counter;";
    assert_eq!(header.matches(typedef).count(), 1, "{header}");
    assert!(
        header.contains("tz_handle_4Main_7Counter tz_pass_through(tz_handle_4Main_7Counter arg0);")
    );
    assert!(header.contains("int64_t tz_borrowed(tz_handle_4Main_7Counter arg0);"));
    assert!(!header.contains("tsuzuri_alloc"));
    // Externs without a link name get prototypes after the typedef.
    let module = analyze(
        "extern type Counter\nextern def make :: i64 -> Counter\nfn use_it(start: i64) -> i64 { let counter = make start; 0 }",
    )
    .unwrap();
    let header = llvm::header(&module);
    let typedef_at = header
        .find("typedef struct tz_handle_4Main_7Counter_s")
        .unwrap();
    let prototype_at = header
        .find("tz_handle_4Main_7Counter tsuzuri_host_Main_make(int64_t arg0);")
        .unwrap();
    assert!(typedef_at < prototype_at);
    // The typedef spelling keeps module boundaries: `A_B.C.H` and `A.B_C.H` differ.
    let module = analyze_modules(&[
        (
            "A_B/C.tz",
            "extern type H\nexport def one :: H -> H\nfn one h = h",
        ),
        (
            "A/B_C.tz",
            "extern type H\nexport def two :: H -> H\nfn two h = h",
        ),
        ("Main.tz", "1"),
    ])
    .unwrap();
    let header = llvm::header(&module);
    assert!(
        header.contains("tz_handle_3A_B_1C_1H") && header.contains("tz_handle_1A_3B_C_1H"),
        "{header}"
    );
}

#[test]
fn handles_are_rejected_inside_abi_records() {
    for body in [
        "record Holder { counter: Counter }\nextern \"e12_holder\" def holder :: Holder -> i64\n1",
        "record Holder { counter: Counter }\nexport def hold :: Holder -> i64\nfn hold h = 1",
        "extern \"e12_mut\" def bad :: ref mut Counter -> unit\n1",
        "extern \"e12_array\" def bad :: [Counter] -> i64\n1",
        "export def bad :: [Counter] -> i64\nfn bad cs = 1",
        "export def bad :: ref mut Counter -> i64\nfn bad c = 1",
    ] {
        assert_eq!(code(&with_handles(body)), "E1008", "{body}");
    }
}

const CALLBACKS: &str = "extern \"e12_apply\" def apply :: (i64 -> i64) -> i64 -> i64
extern \"e12_run\" def run_host :: (unit -> i64) -> i64
extern type Counter
extern \"e12_new\" def counter_new :: i64 -> Counter
extern \"e12_free\" def counter_free :: Counter -> i64
extern \"e12_visit\" def visit :: (ref Counter -> i64) -> ref Counter -> i64
private def inc :: i64 -> i64
fn inc value = value + 1
private def answer :: unit -> i64
fn answer _ = 42
private def peek :: ref Counter -> i64
fn peek counter = 7
private def unused :: i64 -> i64
fn unused value = value
";

#[test]
fn callbacks_lower_to_internal_c_wrappers() {
    let source = format!(
        "{CALLBACKS}export def root :: i64 -> i64\nfn root value =\n    let counter = counter_new value\n    let seen = visit peek (&counter)\n    seen + counter_free counter + apply inc value + apply inc 1 + run_host answer\n"
    );
    let module = analyze(&source).unwrap();
    for wasm in [false, true] {
        let ir = ir(&module, wasm);
        assert!(ir.contains("declare i64 @e12_apply(ptr, i64)"));
        assert!(ir.contains("declare i64 @e12_run(ptr)"));
        assert!(ir.contains("declare i64 @e12_visit(ptr, ptr)"));
        // The host gets the wrappers' addresses; no closure is built and the extern has no body.
        assert!(ir.contains("call i64 @e12_apply(ptr @tz.callback.Main.inc, i64 "));
        assert!(ir.contains("call i64 @e12_run(ptr @tz.callback.Main.answer)"));
        assert!(ir.contains("call i64 @e12_visit(ptr @tz.callback.Main.peek, ptr "));
        assert!(!ir.contains("@tz.fn.Main.apply("));
        for callback in ["inc", "answer", "peek"] {
            assert!(
                !ir.contains(&format!("ptr @tz.desc.Main.{callback}.")),
                "{callback}"
            );
        }
        // Nothing in the program builds, copies, or drops a function value, so the closure runtime
        // (`closure.ll`) is not linked in, and no function builds a closure outside the adapters.
        assert!(!ir.contains("@tz.closure."), "closure.ll is not needed");
        let outside_adapters = ir
            .split("\n\n")
            .filter(|block| !block.contains("define internal ") || !block.contains("@tz.apply."))
            .any(|block| block.contains("insertvalue %tz.closure"));
        assert!(!outside_adapters, "a function value is built");
        // Wrappers are internal and take C scalars. A lone unit input has no C parameter,
        // and a borrowed handle is a pointer to a slot holding the host's handle.
        assert_eq!(
            ir.matches("define internal i64 @tz.callback.Main.inc(i64 %arg0) nounwind")
                .count(),
            1,
            "one wrapper however often the function is passed"
        );
        assert!(ir.contains("define internal i64 @tz.callback.Main.answer() nounwind"));
        assert!(ir.contains("define internal i64 @tz.callback.Main.peek(ptr %arg0) nounwind"));
        // Only what the program passes gets a wrapper.
        assert!(!ir.contains("tz.callback.Main.unused"));
        // Scalars and handles need neither the buffer ABI nor the allocator export.
        assert!(!ir.contains("@tsuzuri_alloc"));
        assert_eq!(ir.matches("%tz.abi.buffer").count(), 1);
    }
    let native = ir(&module, false);
    assert!(!native.contains("wasm-import"));
    // WASM calls the wrappers through the function table the linker exports.
    assert!(ir(&module, true).contains(
        "declare i64 @e12_apply(ptr, i64) \"wasm-import-module\"=\"tsuzuri\" \"wasm-import-name\"=\"e12_apply\""
    ));
    // A program that passes no callback emits no wrapper (and so no table export).
    let module = analyze(&format!(
        "{CALLBACKS}export def root :: i64 -> i64\nfn root value = value\n"
    ))
    .unwrap();
    for wasm in [false, true] {
        assert!(!ir(&module, wasm).contains("tz.callback"));
    }
    // Externs without a link name get prototypes with function-pointer parameters;
    // a unit result is `void` and a lone unit input is `(void)`.
    let module = analyze(
        "extern def apply :: (i64 -> i64) -> i64 -> i64\nextern def run_host :: (unit -> i64) -> i64\nextern def each :: (i64 -> unit) -> i64\nprivate def inc :: i64 -> i64\nfn inc value = value + 1\nprivate def answer :: unit -> i64\nfn answer _ = 42\nprivate def note :: i64 -> unit\nfn note value = ()\nexport def root :: i64 -> i64\nfn root value = apply inc value + run_host answer + each note\n",
    )
    .unwrap();
    let header = llvm::header(&module);
    assert!(
        header.contains("int64_t tsuzuri_host_Main_apply(int64_t (*arg0)(int64_t), int64_t arg1);")
    );
    assert!(header.contains("int64_t tsuzuri_host_Main_run_host(int64_t (*arg0)(void));"));
    assert!(header.contains("int64_t tsuzuri_host_Main_each(void (*arg0)(int64_t));"));
    assert!(!header.contains("tsuzuri_alloc"));
    for wasm in [false, true] {
        let ir = ir(&module, wasm);
        assert!(ir.contains("define internal void @tz.callback.Main.note(i64 %arg0) nounwind"));
        assert!(ir.contains("declare i64 @tsuzuri_host_Main_each(ptr)"));
    }
}

#[test]
fn callbacks_reject_values_lambdas_and_bad_types() {
    let prelude = "extern \"e12_apply\" def apply :: (i64 -> i64) -> i64 -> i64
extern \"e12_other\" def other :: i64 -> i64
extern \"e12_apply_f\" def apply_f :: (f64 -> f64) -> f64 -> f64
private def inc :: i64 -> i64
fn inc value = value + 1
private def ident :: 'a -> 'a
fn ident value = value
";
    let named = "callback arguments must name a top-level user function without captures; move the logic into a 'def' and pass its name";
    let direct = "extern 'apply' takes a callback and must be called directly with all arguments";
    let shape = "extern callback parameters support functions over scalar, unit and extern handle types; buffers, records and nested functions are not supported";
    for (body, message) in [
        // A name of a top-level user function is the only callback.
        ("apply (\\value -> value + 1) 1", named),
        (
            "private def local :: i64 -> i64\nfn local value =\n    let callback = inc\n    apply callback value\n1",
            named,
        ),
        ("apply other 1", named),
        ("apply ident 1", named),
        ("apply_f abs 1.0", named),
        ("apply_f Math.sqrt 4.0", named),
        // The callback extern itself runs only through a direct call with every argument.
        (
            "private def partial :: i64 -> i64\nfn partial value =\n    let bound = apply inc\n    bound value\n1",
            direct,
        ),
        (
            "private def use :: ((i64 -> i64) -> i64 -> i64) -> i64\nfn use f = 1\nuse apply",
            direct,
        ),
    ] {
        let source = format!("{prelude}{body}");
        let error = analyze(&source).unwrap_err();
        assert_eq!(
            (error.code, error.message.as_str()),
            ("E1008", message),
            "{body}"
        );
    }
    // Direct calls pass, including from other functions and tests.
    for body in [
        "apply inc 1",
        "private def twice :: i64 -> i64\nfn twice value = apply inc value\ntwice 1",
        "test \"callback\" =\n    assert (apply inc 1 == 2)\n1",
    ] {
        assert_eq!(code(&format!("{prelude}{body}")), "ok", "{body}");
    }
    for declaration in [
        "extern \"e12_a\" def bad :: (string -> i64) -> i64",
        "extern \"e12_b\" def bad :: (i64 -> [i64]) -> i64",
        "extern \"e12_c\" def bad :: ((i64 -> i64) -> i64) -> i64",
        "extern \"e12_d\" def bad :: (i128 -> i64) -> i64",
        "extern \"e12_e\" def bad :: (ref mut i64 -> i64) -> i64",
        "record P { x: i64 }\nextern \"e12_f\" def bad :: (P -> i64) -> i64",
    ] {
        let error = analyze(&format!("{declaration}\n1")).unwrap_err();
        assert_eq!(
            (error.code, error.message.as_str()),
            ("E1008", shape),
            "{declaration}"
        );
    }
    // Scalars, handles and unit stay accepted; exports never take functions.
    for declaration in [
        "extern \"e12_g\" def ok :: (i64 -> unit) -> i64",
        "extern \"e12_h\" def ok :: (unit -> f64) -> i64",
        "extern \"e12_i\" def ok :: (i64 -> bool -> i32) -> i64",
        "extern type H\nextern \"e12_j\" def ok :: (ref H -> i64) -> i64",
    ] {
        assert_eq!(code(&format!("{declaration}\n1")), "ok", "{declaration}");
    }
    assert_eq!(
        code("export def bad :: (i64 -> i64) -> i64\nfn bad f = 1\n1"),
        "E1008"
    );
}

#[test]
fn callback_externs_borrow_temporary_arguments() {
    let prelude = "record Point { x: i64, y: i64 }
extern type Counter
extern \"e12_text\" def apply_text :: (i64 -> i64) -> ref string -> i64
extern \"e12_point\" def apply_point :: (i64 -> i64) -> ref Point -> i64
extern \"e12_sum\" def apply_sum :: (i64 -> i64) -> ref [i64] -> i64
extern \"e12_new\" def counter_new :: i64 -> Counter
extern \"e12_visit\" def visit :: (ref Counter -> i64) -> ref Counter -> i64
extern \"e12_len\" def text_len :: ref string -> i64
private def inc :: i64 -> i64
fn inc value = value + 1
private def peek :: ref Counter -> i64
fn peek counter = 7
private def label :: own i64 -> string
fn label value = $\"item-{value}\"
const NAME: string = \"abcd\"
";
    for (body, call_prefix, undefined, declaration) in [
        (
            "apply_text inc \"abc\"",
            "call i64 @e12_text(ptr @tz.callback.Main.inc, ptr ",
            "@tz.fn.Main.apply_text(",
            "declare i64 @e12_text(ptr, ptr, i64)",
        ),
        (
            "apply_text inc $\"n={value}\"",
            "call i64 @e12_text(ptr @tz.callback.Main.inc, ptr ",
            "@tz.fn.Main.apply_text(",
            "declare i64 @e12_text(ptr, ptr, i64)",
        ),
        (
            "apply_text inc (label value)",
            "call i64 @e12_text(ptr @tz.callback.Main.inc, ptr ",
            "@tz.fn.Main.apply_text(",
            "declare i64 @e12_text(ptr, ptr, i64)",
        ),
        (
            "apply_text inc NAME",
            "call i64 @e12_text(ptr @tz.callback.Main.inc, ptr ",
            "@tz.fn.Main.apply_text(",
            "declare i64 @e12_text(ptr, ptr, i64)",
        ),
        (
            "apply_point inc (Point { x: value, y: 2 })",
            "call i64 @e12_point(ptr @tz.callback.Main.inc, ptr ",
            "@tz.fn.Main.apply_point(",
            "declare i64 @e12_point(ptr, ptr)",
        ),
        (
            "apply_sum inc [value, 2, 3]",
            "call i64 @e12_sum(ptr @tz.callback.Main.inc, ptr ",
            "@tz.fn.Main.apply_sum(",
            "declare i64 @e12_sum(ptr, ptr, i64)",
        ),
        (
            "visit peek (counter_new value)",
            "call i64 @e12_visit(ptr @tz.callback.Main.peek, ptr ",
            "@tz.fn.Main.visit(",
            "declare i64 @e12_visit(ptr, ptr)",
        ),
        (
            "inc (apply_text inc \"abc\")",
            "call i64 @e12_text(ptr @tz.callback.Main.inc, ptr ",
            "@tz.fn.Main.apply_text(",
            "declare i64 @e12_text(ptr, ptr, i64)",
        ),
    ] {
        let source =
            format!("{prelude}export def root :: own i64 -> i64\nfn root value = {body}\n");
        let module = analyze_with_edition(&source, Edition::E2027).unwrap();
        for wasm in [false, true] {
            let emitted = ir(&module, wasm);
            assert!(emitted.contains(call_prefix), "{body}\n{emitted}");
            assert!(!emitted.contains(undefined), "{body}\n{emitted}");
            assert_eq!(emitted.matches(declaration).count(), 1, "{body}\n{emitted}");
            let outside_adapters = emitted
                .split("\n\n")
                .filter(|block| {
                    !block.contains("define internal ") || !block.contains("@tz.apply.")
                })
                .any(|block| block.contains("insertvalue %tz.closure"));
            assert!(!outside_adapters, "{body}\n{emitted}");
            if wasm {
                assert!(emitted.contains("\"wasm-import-module\"=\"tsuzuri\""));
            }
        }
    }
    // Edition 2026 with explicit `ref` on non-string temporaries lowers through the same direct host call,
    // and the shared E2E fixture compiles in both editions.
    let fixture = std::fs::read_to_string("tests/fixtures/ffi_extensions/Main.tz").unwrap();
    for edition in [Edition::E2026, Edition::E2027] {
        let module = analyze_with_edition(&fixture, edition).unwrap();
        let emitted = ir(&module, false);
        assert!(emitted.contains("call i64 @e12_pair_mix(ptr @tz.callback.Main.triple, ptr "));
        assert!(emitted.contains("call i64 @e12_sum_with(ptr @tz.callback.Main.triple, ptr "));
        assert!(!emitted.contains("@tz.fn.Main.pair_mix("));
        assert!(!emitted.contains("@tz.fn.Main.sum_with("));
        assert!(!emitted.contains("@tz.fn.Main.text_units("));
    }
    // Non-callback externs keep their generated wrapper function.
    let module = analyze_with_edition(
        &format!("{prelude}export def root :: own i64 -> i64\nfn root _value = text_len \"abc\"\n"),
        Edition::E2027,
    )
    .unwrap();
    for wasm in [false, true] {
        let emitted = ir(&module, wasm);
        assert!(
            emitted.contains("call i64 @tz.fn.Main.text_len("),
            "{emitted}"
        );
        assert!(
            emitted.contains("define internal i64 @tz.fn.Main.text_len("),
            "{emitted}"
        );
        assert!(emitted.contains("call i64 @e12_len("), "{emitted}");
    }
    for rejected in [
        "\"abc\" |> apply_text inc",
        "let f = apply_text inc\nf \"abc\"",
    ] {
        let error =
            analyze_with_edition(&format!("{prelude}{rejected}\n"), Edition::E2027).unwrap_err();
        assert_eq!(error.code, "E1008");
        assert!(
            error
                .message
                .contains("must be called directly with all arguments")
        );
    }
}

#[test]
fn console_callbacks_guard_against_calls_after_main_while_libraries_stay_process_wide() {
    let source = "extern \"w_reg\" def register :: (i64 -> i64) -> unit\n\
                  extern \"w_reg2\" def register2 :: (i64 -> i64) -> unit\n\
                  def cb1 :: i64 -> i64 = \\x -> x + 1\n\
                  def cb2 :: i64 -> i64 = \\x -> x + 2\n\
                  def main :: unit -> i32 = \\() -> { register cb1; register2 cb2; 0 }\n";
    let module = analyze_with_edition(source, Edition::E2027).unwrap();
    let console = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert_eq!(console, llvm::emit(&module, llvm::Entry::Console).unwrap());
    assert!(console.contains("@tz.main.done = internal global i32 0, align 4"));
    assert_eq!(
        console
            .matches("define internal void @tz.callback.after_main()")
            .count(),
        1
    );
    assert!(console.contains("store atomic i32 1, ptr @tz.main.done release, align 4"));
    assert_eq!(
        console
            .matches("load atomic i32, ptr @tz.main.done acquire, align 4")
            .count(),
        2
    );

    let library = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert!(!library.contains("@tz.main.done"));
    assert!(!library.contains("@tz.callback.after_main"));

    let wasm = ir(&module, true);
    assert!(!wasm.contains("@tz.main.done"));
    assert!(!wasm.contains("@tz.callback.after_main"));

    let no_cb =
        analyze_with_edition("def main :: unit -> i32 = \\() -> 0\n", Edition::E2027).unwrap();
    let plain_console = llvm::emit(&no_cb, llvm::Entry::Console).unwrap();
    assert!(!plain_console.contains("@tz.main.done"));
}
