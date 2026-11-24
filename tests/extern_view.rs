use tsuzuri::{
    analyze, analyze_with_edition, capabilities,
    diagnostic::DiagnosticKind,
    llvm::{self, Allocator, EmitOptions, Entry, F16Path},
    syntax::Edition,
};

#[test]
fn syntax_and_editions() {
    let ok = "\
record Point { x: i32, y: i32 }
@trusted
extern view def read_pt :: i64 -> Point
@trusted
private extern view def write_pt :: i64 -> ref Point -> unit
let view = 1
view
";
    for edition in [Edition::E2026, Edition::E2027] {
        assert!(analyze_with_edition(ok, edition).is_ok(), "{edition:?}");
    }
    let link_err =
        analyze("record Point { x: i32 }\nextern \"sym\" view def r :: i64 -> Point\n0\n")
            .unwrap_err();
    assert_eq!(link_err.kind, Some(DiagnosticKind::ExternViewLinkName.id()));

    let prop_err = analyze(
        "record Point { x: i32 }\n@trusted no_trap\nextern view def r :: i64 -> Point\n0\n",
    )
    .unwrap_err();
    assert_eq!(
        prop_err.kind,
        Some(DiagnosticKind::TrustedViewProperty.id())
    );

    let count_err =
        analyze("record Point { x: i32 }\n@count(1, i32)\nextern view def r :: i64 -> Point\n0\n")
            .unwrap_err();
    assert_eq!(
        count_err.kind,
        Some(DiagnosticKind::MisplacedCountAttribute.id())
    );
}

#[test]
fn signatures_records_and_call_form() {
    for bad_sig in [
        "record Point { x: i32 }\nextern view def r :: i32 -> Point\n0\n",
        "extern view def r :: i64 -> i64\n0\n",
        "record Point { x: i32 }\nextern view def w :: i64 -> Point -> unit\n0\n",
        "record Point { x: i32 }\nextern view def w :: i64 -> ref Point -> i32\n0\n",
        "record Point { x: i32 }\nextern view def r :: unit -> Point\n0\n",
    ] {
        let err = analyze(bad_sig).unwrap_err();
        assert_eq!(
            err.kind,
            Some(DiagnosticKind::ExternViewSignature.id()),
            "{bad_sig}: {:?}",
            err.kind
        );
    }

    // Phase 1 rejects 8/16-bit and bool on unmarked records; Phase 2 accepts @c_layout records without FnPtr/ZString.
    for bad_rec in [
        "record Narrow { a: i8u, b: i32 }\nextern view def r :: i64 -> Narrow\n0\n",
        "record Flagged { f: bool }\nextern view def r :: i64 -> Flagged\n0\n",
        "record Empty {}\nextern view def r :: i64 -> Empty\n0\n",
        "@c_layout\nrecord HasZ { title: ref ZString }\nextern view def r :: i64 -> HasZ\n0\n",
        "@c_layout\nrecord HasCb { cb: FnPtr<(i32 -> i32)> }\nextern view def r :: i64 -> HasCb\n0\n",
    ] {
        let err = analyze(bad_rec).unwrap_err();
        assert_eq!(
            err.kind,
            Some(DiagnosticKind::ExternViewRecord.id()),
            "{bad_rec}: {:?} ({})",
            err.kind,
            err.message
        );
    }

    let phase2_ok = "\
extern type copy Hwnd
@c_layout(pack = 2)
record Packed { a: i8, b: i16u, hwnd: Hwnd, opt: Maybe<Hwnd> }
@trusted
extern view def read_packed :: i64 -> Packed
@trusted
extern view def write_packed :: i64 -> ref Packed -> unit
0
";
    assert!(analyze(phase2_ok).is_ok());

    // Call form: must be called directly with all arguments.
    for bad_call in [
        "record Point { x: i32 }\n@trusted\nextern view def r :: i64 -> Point\nlet f = r\n0\n",
        "record Point { x: i32 }\n@trusted\nextern view def w :: i64 -> ref Point -> unit\nlet g = w 8\n0\n",
    ] {
        let err = analyze(bad_call).unwrap_err();
        assert_eq!(
            err.kind,
            Some(DiagnosticKind::ExternViewCallForm.id()),
            "{bad_call}: {:?}",
            err.kind
        );
    }
}

#[test]
fn inline_lowering_effects_audit_and_wasm() {
    let source = "\
record Point { x: i32, y: i32 }
@trusted
extern view def read_pt :: i64 -> Point
@trusted
extern view def write_pt :: i64 -> ref Point -> unit

@no_alloc
@no_copy
@terminates
export def copy_pt :: i64 -> i64 -> i32 = \\src dst ->
    let p = read_pt src
    write_pt dst (ref p)
    p.x + p.y
";
    let module = analyze(source).unwrap();
    let options = EmitOptions {
        entry: Entry::Library,
        wasm: false,
        debug_output: false,
        runtime_objects: false,
        wasm_simd128: false,
        allocator: Allocator::System,
        f16: F16Path::Soft,
    };
    let ir = llvm::emit_with_options(&module, options).unwrap();
    assert!(ir.contains("inttoptr i64 "), "{ir}");
    assert!(ir.contains("icmp ne ptr "), "{ir}");
    assert!(ir.contains("and i64 "), "{ir}");
    assert!(!ir.contains("@tz.fn.Main.read_pt"), "{ir}");
    assert!(!ir.contains("@tz.fn.Main.write_pt"), "{ir}");
    let header = llvm::header(&module);
    assert!(!header.contains("read_pt"), "{header}");
    assert!(!header.contains("write_pt"), "{header}");

    // Effects: pure and @no_trap reject calling an extern view.
    let pure_err = analyze(
        "record Point { x: i32 }\n@trusted\nextern view def read_pt :: i64 -> Point\npure def f :: i64 -> i32 = \\a -> (read_pt a).x\n0\n",
    )
    .unwrap_err();
    assert_eq!(pure_err.code, "E1029");

    let notrap_err = analyze(
        "record Point { x: i32 }\n@trusted\nextern view def read_pt :: i64 -> Point\n@no_trap\ndef f :: i64 -> i32 = \\a -> (read_pt a).x\n0\n",
    )
    .unwrap_err();
    assert_eq!(notrap_err.code, "E1031");

    // Audit: view read/write in text and JSON.
    let sources = [capabilities::SourceInfo {
        name: "Main".into(),
        std: false,
        package: None,
        path: "Main.tz".into(),
    }];
    let analysis = capabilities::analyze(&module, &sources).unwrap();
    let report = capabilities::Report::new(None, "demo", &analysis, &sources, &[source]);
    let text = report.render_text();
    assert!(text.contains("read_pt -> view read"), "{text}");
    assert!(text.contains("write_pt -> view write"), "{text}");
    let json = report.render_json(false);
    assert!(json.contains("\"symbol\":null,\"view\":\"read\""), "{json}");
    assert!(
        json.contains("\"symbol\":null,\"view\":\"write\""),
        "{json}"
    );

    // WASM rejects reachable extern view with AbiNativeOnly.
    let wasm_err = llvm::emit_with_options(
        &module,
        EmitOptions {
            wasm: true,
            ..options
        },
    )
    .unwrap_err();
    assert_eq!(wasm_err.kind, Some(DiagnosticKind::AbiNativeOnly.id()));
}
