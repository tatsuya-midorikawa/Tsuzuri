use tsuzuri::{analyze, analyze_modules, llvm};

fn ir(module: &tsuzuri::check::CheckedModule, wasm: bool) -> String {
    let first = llvm::emit_target(module, llvm::Entry::Library, wasm).unwrap();
    assert_eq!(
        first,
        llvm::emit_target(module, llvm::Entry::Library, wasm).unwrap(),
        "IR must be deterministic"
    );
    first
}

#[test]
fn nullable_positions_are_accepted() {
    let source = "extern type H
extern type copy CopyH
record Rect { left: i32, top: i32, right: i32, bottom: i32 }
extern \"w15_take_owned\" def take_owned :: Maybe<H> -> i32
extern \"w15_take_ref\" def take_ref :: Maybe<ref H> -> i32
extern \"w15_take_rect\" def take_rect :: Maybe<ref Rect> -> i32
extern \"w15_make\" def make_h :: i32 -> Maybe<H>
extern \"w15_cb_owned\" def cb_owned :: (Maybe<H> -> i32) -> i32
extern \"w15_cb_ref\" def cb_ref :: (Maybe<ref H> -> i32) -> i32
extern \"w15_take_copy\" def take_copy :: Maybe<CopyH> -> i32
private def on_owned :: own Maybe<H> -> i32
fn on_owned m = match m with | Some _ -> 1i32 | None -> 0i32
private def on_ref :: own Maybe<ref H> -> i32
fn on_ref m = match m with | Some _ -> 2i32 | None -> 0i32
export def check_all :: own CopyH -> i32
fn check_all ch =
    let r = Rect { left: 1i32, top: 2i32, right: 9i32, bottom: 4i32 }
    let _ = take_copy (Some ch)
    let _ = take_copy (Some ch)
    match make_h 1i32 with
    | Some h ->
        let first = take_ref (Some (ref h))
        let second = take_owned (Some h)
        first + second + take_rect (Some (ref r)) + take_rect None + cb_owned on_owned + cb_ref on_ref
    | None -> take_owned None + take_ref None
";
    let module = analyze(source).unwrap();
    let _ = ir(&module, false);

    // Cross-module extern handles work in Maybe<H> and Maybe<ref H> too.
    let cross = analyze_modules(&[
        ("Handles.tz", "extern type Window"),
        (
            "Main.tz",
            "extern \"w15_parent\" def parent_of :: Maybe<ref Handles.Window> -> Maybe<Handles.Window>\n1",
        ),
    ])
    .unwrap();
    let _ = ir(&cross, false);
}

#[test]
fn other_maybe_payloads_keep_their_errors() {
    let prelude = "extern type H\nrecord Rect { x: i32 }\n";
    for bad in [
        "extern \"f\" def bad :: Maybe<i64> -> i32",
        "extern \"f\" def bad :: Maybe<Rect> -> i32",
        "extern \"f\" def bad :: Maybe<string> -> i32",
        "extern \"f\" def bad :: Maybe<ref [i64]> -> i32",
        "extern \"f\" def bad :: Maybe<Maybe<H>> -> i32",
        "extern \"f\" def bad :: ref Maybe<H> -> i32",
        "extern \"f\" def bad :: i32 -> Maybe<ref Rect>",
        "extern \"f\" def bad :: i32 -> Maybe<ref H>",
        "extern \"f\" def bad :: (i32 -> Maybe<H>) -> i32",
        "extern \"f\" def bad :: (Maybe<ref Rect> -> i32) -> i32",
    ] {
        let error = analyze(&format!("{prelude}{bad}\n1")).unwrap_err();
        assert_eq!(
            (error.code, error.kind),
            ("E1008", Some("abi_nullable_payload")),
            "{bad}: {}",
            error.message
        );
    }

    // A user union of the same shape keeps UnsupportedExternSignature / UnsupportedExportSignature.
    let user_opt = analyze(
        "extern type H\nunion Opt<'a> = None | Some of 'a\nextern \"f\" def bad :: Opt<H> -> i32\n1",
    )
    .unwrap_err();
    assert_eq!(
        (user_opt.code, user_opt.kind),
        ("E1008", Some("unsupported_extern_signature"))
    );

    let export_user_opt = analyze(
        "extern type H\nunion Opt<'a> = None | Some of 'a\nexport def bad :: Opt<H> -> i32\nfn bad _m = 0i32\n1",
    )
    .unwrap_err();
    assert_eq!(
        (export_user_opt.code, export_user_opt.kind),
        ("E1008", Some("unsupported_export_signature"))
    );
}

#[test]
fn nullable_lowering_headers_and_wasm_rejection() {
    // Handle-only Maybe externs do not pull in %tz.abi.buffer or tsuzuri_alloc.
    let handles_only = analyze(
        "extern type H
extern def implicit_maybe :: Maybe<ref H> -> Maybe<H>
extern def implicit_cb :: (Maybe<H> -> Maybe<ref H> -> i32) -> i32
private def cb :: own Maybe<H> -> own Maybe<ref H> -> i32
fn cb _a _b = 1i32
export def run :: i32 -> i32
fn run _x =
    let _ = implicit_maybe None
    implicit_cb cb
",
    )
    .unwrap();
    let native_ir = ir(&handles_only, false);
    assert!(!native_ir.contains("tsuzuri_alloc"), "{native_ir}");
    let header = llvm::header(&handles_only);
    assert_eq!(
        header
            .matches("typedef struct tz_handle_4Main_1H_s *tz_handle_4Main_1H;")
            .count(),
        1,
        "{header}"
    );
    assert!(
        header.contains(
            "tz_handle_4Main_1H tsuzuri_host_Main_implicit_maybe(tz_handle_4Main_1H arg0);"
        ),
        "{header}"
    );
    assert!(
        header.contains("int32_t tsuzuri_host_Main_implicit_cb(int32_t (*arg0)(tz_handle_4Main_1H, tz_handle_4Main_1H));"),
        "{header}"
    );

    // Maybe<ref R> emits the record type definition and branches around write_host_record on None.
    let record_maybe = analyze(
        "record Rect { left: i32, right: i32 }
extern def implicit_rect :: Maybe<ref Rect> -> i32
export def run :: i32 -> i32
fn run x =
    let r = Rect { left: 1i32, right: x }
    implicit_rect (Some (ref r)) + implicit_rect None
",
    )
    .unwrap();
    let rect_ir = ir(&record_maybe, false);
    assert!(
        rect_ir.contains("%tz_record_4Main_4Rect = type"),
        "{rect_ir}"
    );
    assert!(rect_ir.contains("phi ptr [ null, %"), "{rect_ir}");
    let rect_header = llvm::header(&record_maybe);
    assert!(
        rect_header.contains(
            "int32_t tsuzuri_host_Main_implicit_rect(const tz_record_4Main_4Rect *arg0);"
        ),
        "{rect_header}"
    );

    // WASM rejects reached nullable externs with abi_native_only, and ignores unreached ones.
    let wasm_err = llvm::emit_target(&handles_only, llvm::Entry::Library, true).unwrap_err();
    assert_eq!(
        (wasm_err.code, wasm_err.kind),
        ("E1008", Some("abi_native_only"))
    );

    let unreached = analyze(
        "extern type H
extern \"w15_unused\" def unused_maybe :: Maybe<H> -> Maybe<H>
export def ok :: i32 -> i32
fn ok x = x + 1i32
",
    )
    .unwrap();
    assert!(ir(&unreached, true).contains("define i32 @tz_ok("));

    // Same C symbol declared with Maybe<H>, Maybe<ref Rect>, and i64 merges to one declare (W14 + W15).
    let w14_combo = analyze(
        "extern type H
record Rect { left: i32, right: i32 }
extern \"SendMessageW\" def send_int :: Maybe<ref H> -> i64 -> i64
extern \"SendMessageW\" def send_rect :: Maybe<ref H> -> Maybe<ref Rect> -> i64
export def both :: i64 -> i64
fn both x =
    let r = Rect { left: 1i32, right: 5i32 }
    send_int None x + send_rect None (Some (ref r))
",
    )
    .unwrap();
    let combo_ir = ir(&w14_combo, false);
    assert_eq!(
        combo_ir
            .matches("declare i64 @SendMessageW(ptr, i64)\n")
            .count(),
        1,
        "{combo_ir}"
    );
}

#[test]
fn nonnull_handle_result_export_and_callback_trap_on_null_on_native_only() {
    use tsuzuri::trap::{TrapKind, TrapSource};
    let source = "extern type H
extern \"ns01_make\" def make_h :: unit -> H
extern \"ns01_visit\" def visit :: (H -> ref H -> i32) -> i32
private def cb :: own H -> ref H -> i32
fn cb _owned _borrowed = 1i32
export def take_owned :: own H -> i32
fn take_owned _h = 1i32
export def take_ref :: ref H -> i32
fn take_ref _h = 2i32
export def call_all :: i32 -> i32
fn call_all _ =
    let _ = make_h ()
    visit cb
";
    let module = analyze(source).unwrap();
    let trap_sources = [TrapSource {
        path: "Main.tz",
        text: source,
    }];
    let options = |wasm| llvm::EmitOptions {
        entry: llvm::Entry::Library,
        wasm,
        debug_output: false,
        runtime_objects: false,
        wasm_simd128: false,
        allocator: llvm::Allocator::System,
        f16: llvm::F16Path::Soft,
    };
    let emitted = llvm::emit_with_trap_info(&module, options(false), &trap_sources).unwrap();
    let null_sites: Vec<_> = emitted
        .trap_sites
        .iter()
        .filter(|site| site.kind == TrapKind::NullHandle)
        .collect();
    // make_h return (1), take_owned param (1), take_ref param (1), cb owned + borrowed params (2) = 5 sites.
    assert!(
        null_sites.len() >= 4,
        "expected NullHandle sites for extern return, exports, and callback: {null_sites:?}\n{}",
        emitted.ir
    );
    assert!(
        emitted.ir.contains("icmp ne ptr %arg0, null"),
        "{}",
        emitted.ir
    );

    // WASM keeps handle 0 valid: no NullHandle guards are emitted.
    let wasm = llvm::emit_with_trap_info(&module, options(true), &trap_sources).unwrap();
    assert!(
        wasm.trap_sites
            .iter()
            .all(|site| site.kind != TrapKind::NullHandle),
        "WASM must not emit NullHandle traps: {:?}",
        wasm.trap_sites
    );
}

#[test]
fn maybe_handle_uses_null_pointer_niche_on_native_only() {
    let source = "extern type H
extern \"ns01_opt\" def opt_h :: Maybe<H> -> Maybe<H>
export def round :: i32 -> i32
fn round _ =
    match opt_h None with
    | Some h ->
        match opt_h (Some h) with
        | Some _ -> 1i32
        | None -> 2i32
    | None -> 0i32
";
    let module = analyze(source).unwrap();
    let native_ir = ir(&module, false);
    // On native, Maybe<H> uses Niche::NullPointer (single ptr), so no union type definition is emitted for Maybe<H>.
    assert!(
        !native_ir.contains("%\"tz.union.Maybe.Maybe[extern.Main.H]\""),
        "{native_ir}"
    );
    assert!(
        native_ir.contains("define internal ptr @tz.fn.Main.opt_h(ptr %arg0)"),
        "{native_ir}"
    );

    // On WASM (without ABI Maybe<H>), Maybe<H> stays a tagged union so handle 0 remains distinct from None.
    let wasm_internal = analyze(
        "extern type H
extern \"ns01_zero\" def zero_h :: unit -> H
export def keep :: i32 -> i32
fn keep _ =
    let m: Maybe<H> = Some (zero_h ())
    match m with
    | Some _ -> 1i32
    | None -> 0i32
",
    )
    .unwrap();
    let wasm_ir = ir(&wasm_internal, true);
    assert!(
        wasm_ir.contains("%\"tz.union.Maybe.Maybe[extern.Main.H]\" = type { i8, ptr }"),
        "{wasm_ir}"
    );
}

#[test]
fn export_nullable_positions_are_accepted() {
    let handles_only = analyze(
        "extern type Item
extern type copy Token
export def pass_item :: own Maybe<Item> -> Maybe<Item>
fn pass_item m = m
export def check_borrowed :: own Maybe<ref Item> -> own Maybe<Token> -> i32
fn check_borrowed a b =
    let s1 = match a with | Some _ -> 10i32 | None -> 0i32
    let s2 = match b with | Some _ -> 5i32 | None -> 0i32
    s1 + s2
",
    )
    .unwrap();
    let native_ir = ir(&handles_only, false);
    assert!(
        !native_ir.contains("tsuzuri_alloc"),
        "handle-only nullable exports must not emit tsuzuri_alloc:\n{native_ir}"
    );
    let header = llvm::header(&handles_only);
    assert!(
        !header.contains("tsuzuri_alloc"),
        "handle-only nullable header must not declare tsuzuri_alloc:\n{header}"
    );
    assert!(
        header.contains("tz_handle_4Main_4Item tz_pass_item(tz_handle_4Main_4Item arg0);"),
        "{header}"
    );
    assert!(
        header.contains(
            "int32_t tz_check_borrowed(tz_handle_4Main_4Item arg0, tz_handle_4Main_5Token arg1);"
        ),
        "{header}"
    );

    let with_record = analyze(
        "extern type Item
record Filter { min_x: i32, max_x: i32 }
extern \"ns02_make_item\" def make_item :: i32 -> Item
export def filter_sum :: own Maybe<ref Filter> -> i32
fn filter_sum mf =
    match mf with
    | Some f -> f.min_x + f.max_x
    | None -> -1i32
export def find_item :: own Maybe<ref Filter> -> Maybe<Item>
fn find_item mf =
    match mf with
    | Some f -> if f.min_x <= f.max_x then Some (make_item f.min_x) else None
    | None -> None
",
    )
    .unwrap();
    let rec_ir = ir(&with_record, false);
    assert!(
        rec_ir.contains("%tz_record_4Main_6Filter = type"),
        "{rec_ir}"
    );
    assert!(rec_ir.contains("phi ptr [ null, %"), "{rec_ir}");
    let rec_header = llvm::header(&with_record);
    assert!(
        rec_header.contains("int32_t tz_filter_sum(const tz_record_4Main_6Filter *arg0);"),
        "{rec_header}"
    );
    assert!(
        rec_header
            .contains("tz_handle_4Main_4Item tz_find_item(const tz_record_4Main_6Filter *arg0);"),
        "{rec_header}"
    );
}

#[test]
fn export_other_maybe_payloads_fail_with_abi_nullable_payload() {
    let prelude = "extern type H\nrecord Rect { x: i32 }\n";
    for bad in [
        "export def bad :: own Maybe<i64> -> i32\nfn bad _ = 0i32",
        "export def bad :: own Maybe<Rect> -> i32\nfn bad _ = 0i32",
        "export def bad :: own Maybe<string> -> i32\nfn bad _ = 0i32",
        "export def bad :: own Maybe<ref [i64]> -> i32\nfn bad _ = 0i32",
        "export def bad :: own Maybe<Maybe<H>> -> i32\nfn bad _ = 0i32",
        "export def bad :: ref Maybe<H> -> i32\nfn bad _ = 0i32",
        "export def bad :: i32 -> Maybe<Rect>\nfn bad _ = None",
        "export def bad :: i32 -> Maybe<ref Rect>\nfn bad _ = None",
        "export def bad :: i32 -> Maybe<ref H>\nfn bad _ = None",
        "export def bad :: i32 -> Maybe<i64>\nfn bad _ = None",
        "export def bad :: i32 -> Maybe<string>\nfn bad _ = None",
    ] {
        let error = analyze(&format!("{prelude}{bad}\n")).unwrap_err();
        assert_eq!(
            (error.code, error.kind),
            ("E1008", Some("abi_nullable_payload")),
            "{bad}: {}",
            error.message
        );
    }
}

#[test]
fn wasm_and_bindings_js_reject_nullable_exports() {
    for src in [
        "extern type H\nexport def f :: own Maybe<H> -> i32\nfn f _ = 0i32\n",
        "extern type H\nexport def f :: own Maybe<ref H> -> i32\nfn f _ = 0i32\n",
        "record R { x: i32 }\nexport def f :: own Maybe<ref R> -> i32\nfn f _ = 0i32\n",
        "extern type H\nexport def f :: i32 -> Maybe<H>\nfn f _ = None\n",
    ] {
        let module = analyze(src).unwrap();
        let wasm_err = llvm::emit_target(&module, llvm::Entry::Library, true).unwrap_err();
        assert_eq!(
            (wasm_err.code, wasm_err.kind),
            ("E1008", Some("abi_native_only")),
            "wasm emit should reject:\n{src}"
        );

        let dir = std::env::temp_dir().join(format!(
            "tz-ns02-js-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let tz_path = dir.join("Main.tz");
        let js_out = dir.join("out.mjs");
        std::fs::write(&tz_path, src).unwrap();
        let project = tsuzuri::driver::Project::load(&tz_path).unwrap();
        let options = tsuzuri::driver::BuildOptions {
            emit: tsuzuri::driver::Emit::BindingsJs,
            target: tsuzuri::driver::Target::Wasm32,
            ..tsuzuri::driver::BuildOptions::default()
        };
        let js_err = tsuzuri::driver::build(&module, &project, &js_out, options).unwrap_err();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            (js_err.code, js_err.kind),
            ("E1008", Some("abi_native_only")),
            "bindings-js should reject:\n{src}"
        );
    }
}

#[test]
fn try_wrapper_and_native_bindings_handle_nullable_exports() {
    use tsuzuri::trap::TrapSource;

    let source = "extern type Item
record Filter { min_x: i32, max_x: i32 }
export def inspect_all :: own Maybe<Item> -> own Maybe<ref Item> -> own Maybe<ref Filter> -> Maybe<Item>
fn inspect_all owned borrowed filt =
    let ok = match filt with
        | Some f ->
            let _ = 100i32 / f.max_x
            f.min_x >= 0i32
        | None -> true
    if !ok then None
    else
        match borrowed with
        | Some _ -> owned
        | None -> owned
";
    let module = analyze(source).unwrap();
    let trap_sources = [TrapSource {
        path: "Main.tz",
        text: source,
    }];
    let options = llvm::EmitOptions {
        entry: llvm::Entry::Library,
        wasm: false,
        wasm_simd128: false,
        debug_output: false,
        runtime_objects: false,
        allocator: llvm::Allocator::System,
        f16: llvm::F16Path::Soft,
    };
    let try_ir = llvm::emit_trap_return(&module, options, &trap_sources, None, false)
        .unwrap()
        .ir;
    assert!(
        try_ir.contains("define i32 @tsuzuri_try_inspect_all(ptr %trap, ptr %result, ptr %arg0, ptr %arg1, ptr %arg2)"),
        "{try_ir}"
    );
    let try_header = llvm::header_with(&module, true);
    assert!(
        try_header.contains(
            "int32_t tsuzuri_try_inspect_all(tsuzuri_trap_info *trap, tz_handle_4Main_4Item *result, tz_handle_4Main_4Item arg0, tz_handle_4Main_4Item arg1, const tz_record_4Main_6Filter *arg2);"
        ),
        "{try_header}"
    );

    for trap_mode in [false, true] {
        let cs = tsuzuri::bindings::csharp(&module, "demo", trap_mode);
        assert!(
            cs.contains("public static tz_handle_4Main_4Item? inspect_all(tz_handle_4Main_4Item? arg0, tz_handle_4Main_4Item? arg1, in tz_record_4Main_6Filter? arg2)"),
            "{cs}"
        );
        assert!(cs.contains("if (arg0 is { Value: 0 })"), "{cs}");
        assert!(
            cs.contains("return value == 0 ? null : new tz_handle_4Main_4Item(value);"),
            "{cs}"
        );

        let py = tsuzuri::bindings::python(&module, "demo", trap_mode);
        assert!(py.contains("if arg0 is not None:"), "{py}");
        assert!(
            py.contains("record2 = None if arg2 is None else _to_tz_record_4Main_6Filter(arg2,"),
            "{py}"
        );
        assert!(py.contains("return None if "), "{py}");

        let cpp = tsuzuri::bindings::cpp(&module, "demo", trap_mode);
        assert!(cpp.contains("#include <optional>\n"), "{cpp}");
        assert!(
            cpp.contains("inline std::optional<tz_handle_4Main_4Item> inspect_all(std::optional<tz_handle_4Main_4Item> arg0, std::optional<tz_handle_4Main_4Item> arg1, const std::optional<tz_record_4Main_6Filter> &arg2)"),
            "{cpp}"
        );
        assert!(
            cpp.contains("if (arg0.has_value() && *arg0 == nullptr) throw std::invalid_argument("),
            "{cpp}"
        );
    }

    // Non-Maybe modules do not gain `#include <optional>` in C++ bindings.
    let plain = analyze("export def add :: i32 -> i32 -> i32\nfn add a b = a + b\n").unwrap();
    let plain_cpp = tsuzuri::bindings::cpp(&plain, "plain", false);
    assert!(!plain_cpp.contains("<optional>"), "{plain_cpp}");
}
