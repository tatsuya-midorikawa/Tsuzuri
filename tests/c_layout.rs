use tsuzuri::{
    analyze, analyze_with_edition, api_diff, bindings,
    diagnostic::DiagnosticKind,
    driver::Project,
    llvm::{self, Allocator, EmitOptions, Entry, F16Path},
    syntax::Edition,
};

fn emit_native(source: &str) -> (String, String) {
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
    let header = llvm::header(&module);
    (ir, header)
}

#[test]
fn attribute_syntax_and_editions() {
    let ok = "@c_layout\nrecord A { x: i32 }\n@c_layout(pack = 1)\nprivate record B { a: i8, b: i32 }\n0\n";
    for edition in [Edition::E2026, Edition::E2027] {
        assert!(analyze_with_edition(ok, edition).is_ok(), "{edition:?}");
    }
    for (source, expected) in [
        (
            "@c_layout\ndef f :: i64 -> i64 = \\x -> x\n0\n",
            DiagnosticKind::MisplacedCLayoutAttribute,
        ),
        (
            "@c_layout\n@c_layout\nrecord A { x: i32 }\n0\n",
            DiagnosticKind::CLayoutRepeated,
        ),
        (
            "@c_layout(pack = 3)\nrecord A { x: i32 }\n0\n",
            DiagnosticKind::CLayoutArgument,
        ),
        (
            "@c_layout(align = 4)\nrecord A { x: i32 }\n0\n",
            DiagnosticKind::CLayoutArgument,
        ),
    ] {
        for edition in [Edition::E2026, Edition::E2027] {
            let err = analyze_with_edition(source, edition).unwrap_err();
            assert_eq!(err.kind, Some(expected.id()), "{source} ({edition:?})");
        }
    }
}

#[test]
fn field_types_validation() {
    let good = "\
extern type OwnedH
extern type copy CopyH
@c_layout
record Inner { a: i8, b: i16u }
@c_layout
record Full {
    x0: i8, x1: i8u, x2: i16, x3: i16u,
    x4: i32, x5: i32u, x6: i64, x7: i64u,
    f0: f32, f1: f64,
    arr: [i16u; 4],
    nested: Inner,
    ch: CopyH,
    opt_c: Maybe<CopyH>,
    opt_o: Maybe<OwnedH>,
    cb: FnPtr<(i32 -> i32)>,
    title: ref ZString,
    opt_title: Maybe<ref ZString>
}
0
";
    assert!(analyze(good).is_ok());

    for (source, expected) in [
        (
            "@c_layout\nrecord Bad { flag: bool }\n0\n",
            DiagnosticKind::CLayoutBoolField,
        ),
        (
            "@c_layout\nrecord Bad { flags: [bool; 2] }\n0\n",
            DiagnosticKind::CLayoutBoolField,
        ),
        (
            "extern type H\n@c_layout\nrecord Bad { h: H }\n0\n",
            DiagnosticKind::CLayoutOwnedHandleField,
        ),
        (
            "@c_layout\nrecord Bad { s: string }\n0\n",
            DiagnosticKind::CLayoutFieldType,
        ),
        (
            "@c_layout\nrecord Bad { p: (i32 * i32) }\n0\n",
            DiagnosticKind::CLayoutFieldType,
        ),
        (
            "record Plain { x: i32 }\n@c_layout\nrecord Bad { p: Plain }\n0\n",
            DiagnosticKind::CLayoutNestedRecord,
        ),
        (
            "@c_layout\nrecord Bad<'a> { x: 'a }\n0\n",
            DiagnosticKind::CLayoutGeneric,
        ),
        (
            "@c_layout\nrecord Bad { r } { x: ref { r } i32 }\n0\n",
            DiagnosticKind::CLayoutGeneric,
        ),
        (
            "@c_layout\nrecord Bad {}\n0\n",
            DiagnosticKind::CLayoutEmpty,
        ),
        (
            "@c_layout\nrecord Bad { x: i32 }\ninstance Drop<Bad> { fn drop _ = () }\n0\n",
            DiagnosticKind::CLayoutDrop,
        ),
        (
            "@c_layout\nrecord Inner { x: i8 }\nrecord Outer { inner: Inner }\nexport def f :: ref Outer -> i8 = \\o -> o.inner.x\n",
            DiagnosticKind::CLayoutInPlainRecord,
        ),
        (
            "@c_layout\nrecord InOnly { title: ref ZString }\nexport def f :: ref InOnly -> i32 = \\_ -> 0i32\n",
            DiagnosticKind::CLayoutInputOnly,
        ),
        (
            "@c_layout\nrecord InOnly { title: ref ZString }\n@trusted\nextern \"c_fn\" def f :: ref mut InOnly -> i32\n0\n",
            DiagnosticKind::CLayoutInputOnly,
        ),
        (
            "@c_layout\nrecord WithCb { cb: FnPtr<(i32 -> i32)> }\n@trusted\nextern \"c_fn\" def f :: unit -> WithCb\n0\n",
            DiagnosticKind::FnptrFromHost,
        ),
    ] {
        let err = analyze(source).unwrap_err();
        assert_eq!(
            err.kind,
            Some(expected.id()),
            "{source}: got {:?} ({})",
            err.kind,
            err.message
        );
    }
}

#[test]
fn dlg_template_and_log_font_layouts() {
    let source = "\
@c_layout
record LogFont {
    height: i32, width: i32, escapement: i32, orientation: i32, weight: i32,
    italic: i8u, underline: i8u, strike_out: i8u, char_set: i8u,
    out_precision: i8u, clip_precision: i8u, quality: i8u, pitch_and_family: i8u,
    face_name: [i16u; 32]
}
@c_layout(pack = 2)
record DlgTemplate {
    style: i32u, extended_style: i32u, cdit: i16u, x: i16, y: i16, cx: i16, cy: i16
}
export def font_h :: ref LogFont -> i32 = \\f -> f.height
export def dlg_items :: ref DlgTemplate -> i16u = \\d -> d.cdit
";
    let (ir, header) = emit_native(source);
    assert!(
        ir.contains(
            "%tz_record_4Main_11DlgTemplate = type <{ i32, i32, i16, i16, i16, i16, i16 }>"
        ),
        "{ir}"
    );
    assert!(header.contains("#include <stddef.h>"), "{header}");
    assert!(header.contains("uint8_t italic;"), "{header}");
    assert!(header.contains("uint16_t face_name[32];"), "{header}");
    assert!(
        header.contains("sizeof(tz_record_4Main_7LogFont) == 92"),
        "{header}"
    );
    assert!(
        header.contains("offsetof(tz_record_4Main_7LogFont, face_name) == 28"),
        "{header}"
    );
    assert!(header.contains("#pragma pack(push, 2)"), "{header}");
    assert!(header.contains("#pragma pack(pop)"), "{header}");
    assert!(
        header.contains("sizeof(tz_record_4Main_11DlgTemplate) == 18"),
        "{header}"
    );
    assert!(
        header.contains("offsetof(tz_record_4Main_11DlgTemplate, cdit) == 8"),
        "{header}"
    );
}

#[test]
fn explicit_padding_and_nested_pack_cap() {
    let source = "\
@c_layout
record Padded { a: i8, b: i32, c: i8 }
@c_layout
record Inner { x: i32, y: i64 }
@c_layout(pack = 1)
record PackedOuter { tag: i8, inner: Inner }
export def read_padded :: ref Padded -> i32 = \\p -> p.b
export def read_packed :: ref PackedOuter -> i64 = \\p -> p.inner.y
";
    let (ir, header) = emit_native(source);
    assert!(
        ir.contains("%tz_record_4Main_6Padded = type <{ i8, [3 x i8], i32, i8, [3 x i8] }>"),
        "{ir}"
    );
    assert!(
        ir.contains("%tz_record_4Main_11PackedOuter = type <{ i8, %tz_record_4Main_5Inner }>"),
        "{ir}"
    );
    assert!(
        header.contains("sizeof(tz_record_4Main_6Padded) == 12"),
        "{header}"
    );
    assert!(
        header.contains("sizeof(tz_record_4Main_11PackedOuter) == 17"),
        "{header}"
    );
    assert!(
        header.contains("offsetof(tz_record_4Main_11PackedOuter, inner) == 1"),
        "{header}"
    );
    // Inside PackedOuter (pack = 1), leaf loads of Inner.x and Inner.y must use align 1.
    assert!(ir.contains("load i64, ptr "), "{ir}");
    assert!(ir.contains(", align 1"), "{ir}");
}

#[test]
fn copy_handles_phase2_and_wasm_checks() {
    let source = "\
extern type copy Hwnd
@c_layout
record WndInfo {
    hwnd: Hwnd,
    parent: Maybe<Hwnd>,
    cb: FnPtr<(Hwnd -> i32)>,
    title: ref ZString,
    menu: Maybe<ref ZString>
}
@trusted
extern \"reg_wnd\" def reg_wnd :: ref WndInfo -> i32
def cb :: Hwnd -> i32 = \\_ -> 1i32
def main :: unit -> i32 = \\() ->
    let _ = cb
    0i32
";
    let module = analyze(source).unwrap();
    let header = llvm::header(&module);
    assert!(
        header.contains("typedef struct tz_handle_4Main_4Hwnd_s *tz_handle_4Main_4Hwnd;"),
        "{header}"
    );
    assert!(
        header.contains("int32_t (*cb)(tz_handle_4Main_4Hwnd);"),
        "{header}"
    );
    assert!(header.contains("const uint16_t *title;"), "{header}");
    assert!(header.contains("const uint16_t *menu;"), "{header}");
    assert!(
        header.contains("sizeof(tz_record_4Main_7WndInfo) == 40"),
        "{header}"
    );

    // WASM rejects pointer-width @c_layout fields with AbiNativeOnly, while numeric-only @c_layout works.
    let ptr_wasm = "\
extern type copy Hwnd
@c_layout
record HasPtr { hwnd: Hwnd }
export def f :: ref HasPtr -> i32 = \\_ -> 0i32
";
    let m = analyze(ptr_wasm).unwrap();
    let err = llvm::emit_with_options(
        &m,
        EmitOptions {
            entry: Entry::Library,
            wasm: true,
            debug_output: false,
            runtime_objects: false,
            wasm_simd128: false,
            allocator: Allocator::System,
            f16: F16Path::Soft,
        },
    )
    .unwrap_err();
    assert_eq!(err.kind, Some(DiagnosticKind::AbiNativeOnly.id()));

    let num_wasm = "\
@c_layout(pack = 2)
record Pair { a: i8, b: i16 }
export def sum :: ref Pair -> i32 = \\p -> (p.a as i32) + (p.b as i32)
";
    let m_num = analyze(num_wasm).unwrap();
    let wasm_ir = llvm::emit_with_options(
        &m_num,
        EmitOptions {
            entry: Entry::Library,
            wasm: true,
            debug_output: false,
            runtime_objects: false,
            wasm_simd128: false,
            allocator: Allocator::System,
            f16: F16Path::Soft,
        },
    )
    .unwrap();
    assert!(wasm_ir.contains("%tz_record_4Main_4Pair = type <{ i8, [1 x i8], i16 }>"));
    // Phase 3 bindings for @c_layout records
    let js = bindings::javascript(&m_num);
    assert!(js.contains("\"c\":true"), "{js}");
    assert!(js.contains("\"pack\":2"), "{js}");
    assert!(js.contains("getInt8"), "{js}");
    let cs = bindings::csharp(&m_num, "demo", false);
    assert!(cs.contains("Pack = 2"), "{cs}");
    assert!(cs.contains("public sbyte a;"), "{cs}");
    assert!(cs.contains("public short b;"), "{cs}");
    let py = bindings::python(&m_num, "demo", false);
    assert!(py.contains("_pack_ = 2"), "{py}");
    assert!(py.contains("_ctypes.c_int8"), "{py}");

    // Phase 2 fixture with FnPtr, ref ZString, and Maybe<ref ZString> must not panic in C#/Python/C++.
    let fixture = std::fs::read_to_string("tests/fixtures/c_layout/Main.tz").unwrap();
    let m_phase2 = analyze(&fixture).unwrap();
    let cs_p2 = bindings::csharp(&m_phase2, "c_layout", false);
    assert!(
        cs_p2.contains("public struct tz_record_4Main_11Phase2Input"),
        "{cs_p2}"
    );
    assert!(cs_p2.contains("public nint cb;"), "{cs_p2}");
    assert!(cs_p2.contains("public nint title;"), "{cs_p2}");
    assert!(cs_p2.contains("public nint opt_menu;"), "{cs_p2}");
    let py_p2 = bindings::python(&m_phase2, "c_layout", false);
    assert!(
        py_p2.contains("class tz_record_4Main_11Phase2Input:"),
        "{py_p2}"
    );
    assert!(py_p2.contains("_ctypes.c_void_p"), "{py_p2}");
    let cpp_p2 = bindings::cpp(&m_phase2, "c_layout", false);
    assert!(!cpp_p2.is_empty());
}

#[test]
fn api_diff_detects_c_layout_changes_as_breaking() {
    let dir = std::env::temp_dir().join(format!("tsuzuri-cl-diff-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let api_of = |label: &str, text: &str| {
        let p = dir.join(label);
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(p.join("Main.tz"), text).unwrap();
        let project = Project::load_for_api(&p).unwrap();
        api_diff::api(&project).unwrap()
    };
    let plain = api_of("plain", "record Pair { a: i8, b: i32 }\n0\n");
    let exact = api_of("exact", "@c_layout\nrecord Pair { a: i8, b: i32 }\n0\n");
    let packed = api_of(
        "packed",
        "@c_layout(pack = 1)\nrecord Pair { a: i8, b: i32 }\n0\n",
    );
    let r1 = api_diff::compare(&plain, &exact);
    assert!(r1.breaking() > 0, "adding @c_layout must be breaking");
    let r2 = api_diff::compare(&exact, &packed);
    assert!(r2.breaking() > 0, "changing pack must be breaking");
    let _ = std::fs::remove_dir_all(&dir);
}
