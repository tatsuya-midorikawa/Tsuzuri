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

    // A user union of the same shape keeps UnsupportedExternSignature.
    let user_opt = analyze(
        "extern type H\nunion Opt<'a> = None | Some of 'a\nextern \"f\" def bad :: Opt<H> -> i32\n1",
    )
    .unwrap_err();
    assert_eq!(
        (user_opt.code, user_opt.kind),
        ("E1008", Some("unsupported_extern_signature"))
    );

    // export def keeps UnsupportedExportSignature.
    let export_err =
        analyze("extern type H\nexport def bad :: Maybe<H> -> i32\nfn bad _m = 0i32\n1")
            .unwrap_err();
    assert_eq!(
        (export_err.code, export_err.kind),
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
