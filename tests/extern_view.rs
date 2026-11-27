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
extern view def read_pt :: ExternAddr -> Point
@trusted
private extern view def write_pt :: ExternAddr -> ref Point -> unit
let view = 1
view
";
    for edition in [Edition::E2026, Edition::E2027] {
        assert!(analyze_with_edition(ok, edition).is_ok(), "{edition:?}");
    }
    let link_err =
        analyze("record Point { x: i32 }\nextern \"sym\" view def r :: ExternAddr -> Point\n0\n")
            .unwrap_err();
    assert_eq!(link_err.kind, Some(DiagnosticKind::ExternViewLinkName.id()));

    let prop_err = analyze(
        "record Point { x: i32 }\n@trusted no_trap\nextern view def r :: ExternAddr -> Point\n0\n",
    )
    .unwrap_err();
    assert_eq!(
        prop_err.kind,
        Some(DiagnosticKind::TrustedViewProperty.id())
    );

    let count_err = analyze(
        "record Point { x: i32 }\n@count(1, i32)\nextern view def r :: ExternAddr -> Point\n0\n",
    )
    .unwrap_err();
    assert_eq!(
        count_err.kind,
        Some(DiagnosticKind::MisplacedCountAttribute.id())
    );
}

#[test]
fn signatures_records_and_call_form() {
    for bad_sig in [
        "record Point { x: i32 }\nextern view def r :: i64 -> Point\n0\n",
        "record Point { x: i32 }\nextern view def w :: i64 -> ref Point -> unit\n0\n",
        "record Point { x: i32 }\nextern view def r :: i32 -> Point\n0\n",
        "extern view def r :: ExternAddr -> i64\n0\n",
        "record Point { x: i32 }\nextern view def w :: ExternAddr -> Point -> unit\n0\n",
        "record Point { x: i32 }\nextern view def w :: ExternAddr -> ref Point -> i32\n0\n",
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
        "record Narrow { a: i8u, b: i32 }\nextern view def r :: ExternAddr -> Narrow\n0\n",
        "record Flagged { f: bool }\nextern view def r :: ExternAddr -> Flagged\n0\n",
        "record Empty {}\nextern view def r :: ExternAddr -> Empty\n0\n",
        "extern view def r :: ExternAddr -> ExternAddr\n0\n",
        "@c_layout\nrecord HasZ { title: ref ZString }\nextern view def r :: ExternAddr -> HasZ\n0\n",
        "@c_layout\nrecord HasCb { cb: FnPtr<(i32 -> i32)> }\nextern view def r :: ExternAddr -> HasCb\n0\n",
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
extern view def read_packed :: ExternAddr -> Packed
@trusted
extern view def write_packed :: ExternAddr -> ref Packed -> unit
0
";
    assert!(analyze(phase2_ok).is_ok());

    // Call form: must be called directly with all arguments.
    for bad_call in [
        "record Point { x: i32 }\n@trusted\nextern view def r :: ExternAddr -> Point\nlet f = r\n0\n",
        "record Point { x: i32 }\n@trusted\nextern view def w :: ExternAddr -> ref Point -> unit\nlet ea = Maybe.get (ExternAddr.of_i64 8)\nlet g = w ea\n0\n",
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
extern view def read_pt :: ExternAddr -> Point
@trusted
extern view def write_pt :: ExternAddr -> ref Point -> unit

@no_alloc
@no_copy
@terminates
export def copy_pt :: i64 -> i64 -> i32 = \\src dst ->
    match (ExternAddr.of_i64 src, ExternAddr.of_i64 dst) with
    | (Maybe.Some s, Maybe.Some d) ->
        let p = read_pt s
        write_pt d (ref p)
        p.x + p.y
    | _ -> -1i32
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
    assert!(!header.contains("ExternAddr"), "{header}");

    // Effects: pure and @no_trap reject calling an extern view.
    let pure_err = analyze(
        "record Point { x: i32 }\n@trusted\nextern view def read_pt :: ExternAddr -> Point\npure def f :: ExternAddr -> i32 = \\a -> (read_pt a).x\n0\n",
    )
    .unwrap_err();
    assert_eq!(pure_err.code, "E1029");

    let notrap_err = analyze(
        "record Point { x: i32 }\n@trusted\nextern view def read_pt :: ExternAddr -> Point\n@no_trap\ndef f :: ExternAddr -> i32 = \\a -> (read_pt a).x\n0\n",
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

#[test]
fn temporary_records_reach_extern_view_writes() {
    let header = "\
@c_layout
record P { x: i32, y: i32 }

@trusted
extern view def write_p :: ExternAddr -> ref P -> unit

def make :: i32 -> P = \\n -> P { x: n, y: n + 1i32 }
";
    let options = EmitOptions {
        entry: Entry::Library,
        wasm: false,
        debug_output: false,
        runtime_objects: false,
        wasm_simd128: false,
        allocator: Allocator::System,
        f16: F16Path::Soft,
    };
    let emit_ed = |edition: Edition, extra_top: &str, body: &str| -> String {
        let source = format!(
            "{header}{extra_top}\ndef f :: ExternAddr -> i32 = \\addr ->\n{body}\n    0i32\n"
        );
        let module = analyze_with_edition(&source, edition).expect(&source);
        let ir = llvm::emit_with_options(&module, options).expect(&source);
        let ir2 = llvm::emit_with_options(&module, options).expect(&source);
        assert_eq!(ir, ir2, "deterministic IR for:\n{source}");
        ir
    };
    let emit = |extra_top: &str, body: &str| emit_ed(Edition::LATEST, extra_top, body);

    for edition in [Edition::E2026, Edition::E2027, Edition::LATEST] {
        let _ = emit_ed(
            edition,
            "",
            "    write_p addr (ref (P { x: 1i32, y: 2i32 }))",
        );
    }

    let t1 = emit("", "    write_p addr (ref (P { x: 1i32, y: 2i32 }))");
    let _t2 = emit("", "    write_p addr (P { x: 1i32, y: 2i32 })");
    let _t3 = emit("", "    write_p addr (ref (make 3i32))");
    let _t4 = emit(
        "",
        "    let p = P { x: 1i32, y: 2i32 }\n    write_p addr (ref { p with x = 5i32 })",
    );
    let _t5 = emit(
        "",
        "    let c = true\n    write_p addr (ref (if c then P { x: 1i32, y: 2i32 } else P { x: 3i32, y: 4i32 }))",
    );
    let t6 = emit(
        "",
        "    let p = P { x: 1i32, y: 2i32 }\n    write_p addr (ref p)",
    );
    let t7 = emit(
        "const ORIGIN: P = P { x: 0i32, y: 0i32 }\n",
        "    write_p addr (ref ORIGIN)",
    );

    let t1_lines: Vec<&str> = t1.lines().collect();
    let t6_lines: Vec<&str> = t6.lines().collect();
    assert_eq!(
        t1_lines.len(),
        t6_lines.len() + 1,
        "t1 vs t6 line count:\n--- t1 ---\n{t1}\n--- t6 ---\n{t6}"
    );
    let zero_stores = |ir: &str| {
        ir.lines()
            .filter(|line| {
                line.trim()
                    .starts_with("store %tz.record.Main.P zeroinitializer, ptr ")
            })
            .count()
    };
    assert_eq!(
        zero_stores(&t1),
        1,
        "expected exactly one temporary cleanup store in t1:\n{t1}"
    );
    assert_eq!(
        zero_stores(&t6),
        0,
        "local variable borrow in t6 must not emit temporary cleanup:\n{t6}"
    );
    assert_eq!(
        zero_stores(&t7),
        0,
        "folded constant write must not emit temporary operand cleanup:\n{t7}"
    );

    let t1_source = format!(
        "{header}\nexport def f :: i64 -> i32 = \\a ->\n    match ExternAddr.of_i64 a with\n    | Maybe.Some addr ->\n        write_p addr (ref (P {{ x: 1i32, y: 2i32 }}))\n        0i32\n    | Maybe.None -> -1i32\n"
    );
    let t1_module = analyze(&t1_source).unwrap();
    let wasm_err = llvm::emit_with_options(
        &t1_module,
        EmitOptions {
            wasm: true,
            ..options
        },
    )
    .unwrap_err();
    assert_eq!(wasm_err.kind, Some(DiagnosticKind::AbiNativeOnly.id()));
}

#[test]
fn extern_addr_rejects_zero_and_view_requires_extern_addr() {
    // Passing raw `i64` directly to an `extern view` fails with E1003 type mismatch.
    let raw_arg_err = analyze(
        "record Point { x: i32, y: i32 }\n@trusted\nextern view def read_pt :: ExternAddr -> Point\ndef f :: i64 -> i32 = \\lparam -> (read_pt lparam).x\n0\n",
    )
    .unwrap_err();
    assert_eq!(raw_arg_err.code, "E1003");

    // `ExternAddr` has no `Default` instance (so `0` address cannot be synthesized via `Default.default ()`).
    let default_err = analyze("let bad: ExternAddr = Default.default ()\n0\n").unwrap_err();
    assert_eq!(default_err.code, "E1005", "{default_err:?}");

    // `ExternAddr` is opaque: direct construction and `.raw` field access are rejected with E1022.
    for opaque_bad in [
        "let bad = ExternAddr { raw: 0 }\n0\n",
        "let a = Maybe.get (ExternAddr.of_i64 8)\nlet r = a.raw\nr\n",
    ] {
        let err = analyze(opaque_bad).unwrap_err();
        assert_eq!(err.code, "E1022", "{opaque_bad}: {err:?}");
    }

    // `ExternAddr` cannot cross `export def` or `extern def` directly as a C struct.
    for abi_bad in [
        "export def bad :: ExternAddr -> i64 = \\a -> ExternAddr.to_i64 a\n",
        "export def bad :: i64 -> ExternAddr = \\x -> Maybe.get (ExternAddr.of_i64 x)\n",
        "extern \"c_fn\" def bad :: ExternAddr -> i64\n0\n",
    ] {
        let err = analyze(abi_bad).unwrap_err();
        assert_eq!(err.code, "E1008", "{abi_bad}: {err:?}");
    }

    // All five `ExternAddr` operations satisfy `pure`, `@no_alloc`, `@no_copy`, `@no_trap`, and `@terminates` with zero warnings.
    let pure_all = "\
@no_alloc
@no_copy
@no_trap
@terminates
pure def exercise :: i64 -> i64u -> i64 = \\a b ->
    match (ExternAddr.of_i64 a, ExternAddr.of_i64u b) with
    | (Maybe.Some x, Maybe.Some y) ->
        match ExternAddr.offset (-8) x with
        | Maybe.Some z -> ExternAddr.to_i64 z + (ExternAddr.to_i64u y as i64)
        | Maybe.None -> 0
    | _ -> 0
exercise 4096 8192i64u
";
    let m =
        analyze(pure_all).expect("all ExternAddr functions are pure/no_alloc/no_trap/terminates");
    assert!(
        m.warnings.is_empty(),
        "unexpected warnings: {:?}",
        m.warnings
    );
}
