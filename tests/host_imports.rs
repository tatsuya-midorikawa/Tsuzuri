use tsuzuri::{analyze, parser};

#[test]
fn parses_extern_signatures_without_bodies() {
    let program =
        parser::parse("extern def host_now :: unit -> i64\nprivate extern def random :: f64\n42")
            .unwrap();
    assert_eq!(program.externs.len(), 2);
    for (source, code) in [
        ("extern fn host() -> i64 { 1 }", "E0002"),
        ("extern def rec host :: i64", "E0002"),
        ("export extern def host :: i64", "E1008"),
        ("extern export def host :: i64", "E1008"),
        ("extern def host :: i64\nfn host() -> i64 { 1 }", "E1001"),
        (
            "extern def host :: i64\ndef host :: i64\nfn host = 1",
            "E1001",
        ),
        ("let extern = 1\nextern", "E0002"),
    ] {
        assert_eq!(parser::parse(source).unwrap_err().code, code, "{source}");
    }
    assert_eq!(
        tsuzuri::analyze_modules(&[("Types.tt", "extern def host :: i64")])
            .unwrap_err()
            .code,
        "E1018"
    );
    assert_eq!(
        analyze("extern def duplicate :: i64\nextern def duplicate :: i64")
            .unwrap_err()
            .code,
        "E1001"
    );
}

#[test]
fn scalar_imports_use_effectful_abi_wrappers_and_prune_unused_declarations() {
    use tsuzuri::llvm;
    let source = "extern def now :: unit -> i64\nextern def unused :: i64\nextern def log :: i64 -> unit\nprivate extern def flag :: bool -> bool\nfn use_host() -> i64 { let value = now (); log value; if flag true then value else 0 }\nfn apply(callback: unit -> i64) -> i64 { callback () }\nfn indirect() -> i64 { apply now }";
    let module = analyze(source).unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert!(ir.contains("declare i64 @tsuzuri_host_Main_now()"));
        assert!(ir.contains("declare void @tsuzuri_host_Main_log(i64)"));
        assert!(ir.contains("declare i32 @tsuzuri_host_Main_flag(i32)"));
        assert!(!ir.contains("tsuzuri_host_Main_unused"));
        if wasm {
            assert!(ir.contains("\"wasm-import-name\"=\"Main.now\""));
        }
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
        );
        for line in ir
            .lines()
            .filter(|line| line.contains("tsuzuri_host_Main_"))
        {
            assert!(
                !line.contains("readnone")
                    && !line.contains("readonly")
                    && !line.contains("speculatable")
            );
        }
    }
    assert!(llvm::header(&module).contains("int64_t tsuzuri_host_Main_now(void)"));
    for (source, code) in [
        ("extern def identity :: 'a -> 'a", "E1015"),
        ("extern def display :: Display<'a> => 'a -> unit", "E1008"),
        ("extern def bad :: string -> unit", "E1008"),
        ("extern def bad :: i128 -> i64", "E1008"),
        ("extern def bad :: (i64 -> i64) -> i64", "E1008"),
        ("extern def bad :: &mut i64 -> unit", "E1008"),
        ("extern def now :: i64\nconst Bad: i64 = now()", "E1026"),
    ] {
        assert_eq!(analyze(source).unwrap_err().code, code, "{source}");
    }
    let error = tsuzuri::analyze_modules(&[
        ("Host.tz", "private extern def now :: i64"),
        ("Main.tz", "Host.now()"),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1022");
}

#[test]
fn host_imports_reuse_buffer_and_record_abi() {
    use tsuzuri::llvm;
    let source = "record Point { x: f64, flag: bool }\nextern def transform :: ref [i64] -> ref string -> Point -> [i64]\nextern def point :: ref Point -> Point\nextern def text :: utf8string\nfn use_buffers() -> i64 { let values = [20, 22]; let label = \"text\"; let point = Point { x: 1.0, flag: true }; let result = transform (&values) (&label) point; let other = Main.point (&point); let text = Main.text(); result.length + text.length }";
    let module = analyze(source).unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert!(
            ir.contains("declare void @tsuzuri_host_Main_transform(ptr, ptr, i64, ptr, i64, ptr)")
        );
        assert!(ir.contains("declare void @tsuzuri_host_Main_point(ptr, ptr)"));
        assert!(ir.contains("@tsuzuri_alloc"));
    }
    assert!(llvm::header(&module).contains("tsuzuri_i64_buffer *out"));
    let formatted =
        tsuzuri::formatter::format_source("Main.tz", source, tsuzuri::syntax::SourceKind::Code)
            .unwrap();
    analyze(&formatted.formatted).unwrap();
}

#[test]
fn import_native_names_do_not_alias_and_unused_imports_stay_absent() {
    let error = tsuzuri::analyze_modules(&[
        ("A_B/C.tz", "extern def call :: i64"),
        ("A/B_C.tz", "extern def call :: i64"),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1001");
    assert_eq!(
        analyze("extern def borrowed :: ref {r} string -> i64")
            .unwrap_err()
            .code,
        "E1013"
    );
    let module =
        analyze("extern def unused :: [i64]\nexport def answer :: i64\nfn answer = 42").unwrap();
    let ir = tsuzuri::llvm::emit_target(&module, tsuzuri::llvm::Entry::Library, true).unwrap();
    assert!(!ir.contains("wasm-import-name") && !ir.contains("tsuzuri_host_Main_unused"));
}
