use tsuzuri::diagnostic::DiagnosticKind;
use tsuzuri::{analyze, llvm};

fn emit_ir(source: &str) -> String {
    let module = analyze(source).expect("source should analyze");
    let a = llvm::emit_target(&module, llvm::Entry::Console, false).expect("IR should emit");
    let b = llvm::emit_target(&module, llvm::Entry::Console, false).expect("IR deterministic");
    assert_eq!(a, b);
    a
}

#[test]
fn types_and_opacity() {
    let valid = r#"
def main :: unit -> i32 = \() ->
    let f: ref string -> Result<ZString, ZString.Error> = ZString.of
    let g: ref ZString -> string = ZString.to_string
    let input = "hi"
    match f (ref input) with
    | Result.Ok z -> g (ref z) |> String.length |> (\n -> n as i32)
    | Result.Error _ -> -1i32
"#;
    analyze(valid).expect("ZString.of and ZString.to_string types match");

    for bad in [
        "def main :: unit -> i32 = \\() -> { let _z = ZString { units: [0i16u] }; 0i32 }\n",
        "def main :: unit -> i32 = \\() -> { let z = ZString.literal \"a\"; z.units.length as i32 }\n",
    ] {
        let err = analyze(bad).unwrap_err();
        assert_eq!(err.code, "E1022", "{bad}\n{err:?}");
    }
}

#[test]
fn literal_is_static_deduplicated_and_zero_alloc() {
    let source = r#"
@trusted pure no_alloc
extern "lstrlenW" def lstrlen :: ref ZString -> i32

def use_literals :: unit -> i32 = \() ->
    let a = ZString.literal "Tsuzuri"
    let b = ZString.literal "Tsuzuri"
    lstrlen a + lstrlen b

def main :: unit -> i32 = \() -> use_literals ()
"#;
    let ir = emit_ir(source);
    let data_defs: Vec<&str> = ir
        .lines()
        .filter(|line| line.contains("private unnamed_addr constant [8 x i16]"))
        .collect();
    assert_eq!(
        data_defs.len(),
        1,
        "identical ZString.literal strings must share one static buffer:\n{ir}"
    );
    assert!(
        data_defs[0]
            .contains("[i16 84, i16 115, i16 117, i16 122, i16 117, i16 114, i16 105, i16 0]"),
        "{}",
        data_defs[0]
    );

    let fn_start = ir
        .find("define internal i32 @tz.fn.Main.use_literals(")
        .unwrap();
    let fn_end = ir[fn_start..].find("\n}\n").unwrap() + fn_start;
    let body = &ir[fn_start..fn_end];
    assert!(!body.contains("@tz.alloc"), "{body}");
}

#[test]
fn literal_rejections() {
    let cases: &[(&str, &str, DiagnosticKind)] = &[
        (
            "def main :: unit -> i32 = \\() -> { let s = \"hi\"; let _z = ZString.literal s; 0i32 }\n",
            "E1005",
            DiagnosticKind::ZstringLiteralArgument,
        ),
        (
            "const S: string = \"hi\"\ndef main :: unit -> i32 = \\() -> { let _z = ZString.literal S; 0i32 }\n",
            "E1005",
            DiagnosticKind::ZstringLiteralArgument,
        ),
        (
            "def main :: unit -> i32 = \\() -> { let x = 1; let _z = ZString.literal $\"a{x}\"; 0i32 }\n",
            "E1005",
            DiagnosticKind::ZstringLiteralArgument,
        ),
        (
            "def mk :: unit -> string = \\() -> \"hi\"\ndef main :: unit -> i32 = \\() -> { let _z = ZString.literal (mk ()); 0i32 }\n",
            "E1005",
            DiagnosticKind::ZstringLiteralArgument,
        ),
        (
            "def main :: unit -> i32 = \\() -> { let _f = ZString.literal; 0i32 }\n",
            "E1005",
            DiagnosticKind::ZstringLiteralArgument,
        ),
        (
            "def main :: unit -> i32 = \\() -> { let _z = \"hi\" |> ZString.literal; 0i32 }\n",
            "E1005",
            DiagnosticKind::ZstringLiteralArgument,
        ),
        (
            "def main :: unit -> i32 = \\() -> { let _z = ZString.literal \"a\\0b\"; 0i32 }\n",
            "E1005",
            DiagnosticKind::ZstringLiteralInteriorNul,
        ),
    ];
    for (idx, (source, code, kind)) in cases.iter().enumerate() {
        let err = analyze(source).expect_err(&format!("case #{idx} should fail"));
        assert_eq!(err.code, *code, "case #{idx}:\n{err:?}");
        assert_eq!(err.kind, Some(kind.id()), "case #{idx}:\n{err:?}");
    }
}

#[test]
fn ref_zstring_and_maybe_ref_zstring_are_one_pointer() {
    let source = r#"
@trusted
extern "lstrlenW" def lstrlen :: ref ZString -> i32

@trusted
extern "opt_zlen" def opt_zlen :: Maybe<ref ZString> -> i32

extern def host_z :: ref ZString -> Maybe<ref ZString> -> i32

export def keep :: i32 -> i32
fn keep x = x

def main :: unit -> i32 = \() ->
    let z = ZString.literal "abc"
    lstrlen z + opt_zlen (Maybe.Some z) + opt_zlen Maybe.None
"#;
    let module = analyze(source).unwrap();
    let ir = llvm::emit_target(&module, llvm::Entry::Console, false).unwrap();
    assert!(ir.contains("declare i32 @lstrlenW(ptr)"), "{ir}");
    assert!(ir.contains("declare i32 @opt_zlen(ptr)"), "{ir}");

    let header = llvm::header(&module);
    assert!(
        header.contains(
            "int32_t tsuzuri_host_Main_host_z(const uint16_t *arg0, const uint16_t *arg1);"
        ),
        "{header}"
    );
    assert!(
        !header.contains("ZString"),
        "opaque ZString must not emit a C struct typedef:\n{header}"
    );
}

#[test]
fn zstring_abi_rejections() {
    let cases: &[(&str, &str, DiagnosticKind)] = &[
        (
            "extern \"f\" def f :: ZString -> i32\ndef main :: unit -> i32 = \\() -> 0i32\n",
            "E1008",
            DiagnosticKind::UnsupportedExternSignature,
        ),
        (
            "extern \"f\" def f :: i32 -> ZString\ndef main :: unit -> i32 = \\() -> 0i32\n",
            "E1008",
            DiagnosticKind::UnsupportedExternSignature,
        ),
        (
            "extern \"f\" def f :: ref mut ZString -> i32\ndef main :: unit -> i32 = \\() -> 0i32\n",
            "E1008",
            DiagnosticKind::UnsupportedExternSignature,
        ),
        (
            "extern \"f\" def f :: (ref ZString -> i32) -> i32\ndef main :: unit -> i32 = \\() -> 0i32\n",
            "E1008",
            DiagnosticKind::UnsupportedExternSignature,
        ),
        (
            "export def f :: ref ZString -> i32\nfn f _z = 0i32\n",
            "E1008",
            DiagnosticKind::UnsupportedExportSignature,
        ),
        (
            "@count(1, i32)\nextern \"f\" def f :: ref ZString -> i32\ndef main :: unit -> i32 = \\() -> 0i32\n",
            "E1008",
            DiagnosticKind::CountNotBuffer,
        ),
    ];
    for (idx, (source, code, kind)) in cases.iter().enumerate() {
        let err = analyze(source).expect_err(&format!("case #{idx} should fail"));
        assert_eq!(err.code, *code, "case #{idx}:\n{err:?}");
        assert_eq!(err.kind, Some(kind.id()), "case #{idx}:\n{err:?}");
    }
}

#[test]
fn wasm_rejects_ref_zstring_extern_while_pure_zstring_module_works() {
    let bad = "extern \"lstrlenW\" def lstrlen :: ref ZString -> i32\nexport def run :: i32 -> i32\nfn run _x = lstrlen (ZString.literal \"hi\")\n";
    let bad_mod = analyze(bad).unwrap();
    let err = llvm::emit_target(&bad_mod, llvm::Entry::Library, true).unwrap_err();
    assert_eq!(err.code, "E1008");
    assert_eq!(err.kind, Some(DiagnosticKind::AbiNativeOnly.id()));

    let pure_wasm = r#"
export def check_z :: i64 -> i64
fn check_z _x =
    let lit = ZString.literal "Tsuzuri"
    let s = ZString.to_string lit
    match ZString.of (ref s) with
    | Result.Ok z -> (ZString.to_string (ref z)).length
    | Result.Error _ -> -1
"#;
    let ok_mod = analyze(pure_wasm).unwrap();
    llvm::emit_target(&ok_mod, llvm::Entry::Library, true)
        .expect("ZString functions without extern must emit cleanly on WASM");
}

#[test]
fn no_alloc_accepts_literal_and_rejects_zstring_of() {
    let ok_src = r#"
@trusted no_alloc
extern "lstrlenW" def lstrlen :: ref ZString -> i32

@no_alloc
def len :: unit -> i32
fn len _unit = lstrlen (ZString.literal "Tsuzuri")

def main :: unit -> i32 = \() -> len ()
"#;
    analyze(ok_src).expect("@no_alloc accepts ZString.literal");

    let bad_src = r#"
@no_alloc
def make_bad :: ref string -> Result<ZString, ZString.Error>
fn make_bad s = ZString.of s

def main :: unit -> i32 = \() -> 0i32
"#;
    let err = analyze(bad_src).expect_err("@no_alloc must reject ZString.of");
    assert_eq!(err.code, "E1031", "{err:?}");
}
