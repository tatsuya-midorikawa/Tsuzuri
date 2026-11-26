//! M06 Phase 2: `ZUtf8String` (`of`, `literal`, `to_utf8string`, `ref ZUtf8String`, `Maybe<ZUtf8String>`).

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
    let f: ref utf8string -> Result<ZUtf8String, ZUtf8String.Error> = ZUtf8String.of
    let g: ref ZUtf8String -> utf8string = ZUtf8String.to_utf8string
    let input = u8"hi"
    match f (ref input) with
    | Result.Ok z -> g (ref z) |> Utf8String.length |> (\n -> n as i32)
    | Result.Error _ -> -1i32
"#;
    analyze(valid).expect("ZUtf8String.of and ZUtf8String.to_utf8string types match");

    for bad in [
        "def main :: unit -> i32 = \\() -> { let _z = ZUtf8String { bytes: [0i8u] }; 0i32 }\n",
        "def main :: unit -> i32 = \\() -> { let z = ZUtf8String.literal \"a\"; z.bytes.length as i32 }\n",
    ] {
        let err = analyze(bad).unwrap_err();
        assert_eq!(err.code, "E1022", "{bad}\n{err:?}");
    }
}

#[test]
fn literal_is_static() {
    let source = r#"
@trusted pure no_alloc
extern "strlen" def strlen :: ref ZUtf8String -> i64

@no_alloc
def use_literals :: unit -> i64 = \() ->
    let a = ZUtf8String.literal "Tsuzuri"
    let b = ZUtf8String.literal "Tsuzuri"
    strlen a + strlen b

def main :: unit -> i32 = \() -> use_literals () as i32
"#;
    let ir = emit_ir(source);
    let data_defs: Vec<&str> = ir
        .lines()
        .filter(|line| line.contains("private unnamed_addr constant [8 x i8]"))
        .collect();
    assert_eq!(
        data_defs.len(),
        1,
        "identical ZUtf8String.literal strings must share one static buffer:\n{ir}"
    );
    assert!(
        data_defs[0].contains("c\"Tsuzuri\\00\", align 1"),
        "{}",
        data_defs[0]
    );

    let fn_start = ir
        .find("define internal i64 @tz.fn.Main.use_literals(")
        .unwrap();
    let fn_end = ir[fn_start..].find("\n}\n").unwrap() + fn_start;
    let body = &ir[fn_start..fn_end];
    assert!(!body.contains("@tz.alloc"), "{body}");
}

#[test]
fn literal_rejections() {
    let cases: &[(&str, &str, DiagnosticKind)] = &[
        (
            "def main :: unit -> i32 = \\() -> { let s = \"hi\"; let _z = ZUtf8String.literal s; 0i32 }\n",
            "E1005",
            DiagnosticKind::Zutf8stringLiteralArgument,
        ),
        (
            "const S: string = \"hi\"\ndef main :: unit -> i32 = \\() -> { let _z = ZUtf8String.literal S; 0i32 }\n",
            "E1005",
            DiagnosticKind::Zutf8stringLiteralArgument,
        ),
        (
            "def main :: unit -> i32 = \\() -> { let x = 1; let _z = ZUtf8String.literal $\"a{x}\"; 0i32 }\n",
            "E1005",
            DiagnosticKind::Zutf8stringLiteralArgument,
        ),
        (
            "def main :: unit -> i32 = \\() -> { let _f = ZUtf8String.literal; 0i32 }\n",
            "E1005",
            DiagnosticKind::Zutf8stringLiteralArgument,
        ),
        (
            "def main :: unit -> i32 = \\() -> { let _z = \"hi\" |> ZUtf8String.literal; 0i32 }\n",
            "E1005",
            DiagnosticKind::Zutf8stringLiteralArgument,
        ),
        (
            "def main :: unit -> i32 = \\() -> { let _z = ZUtf8String.literal \"a\\0b\"; 0i32 }\n",
            "E1005",
            DiagnosticKind::Zutf8stringLiteralInteriorNul,
        ),
        (
            "def main :: unit -> i32 = \\() -> { let _z = ZUtf8String.literal \"a\\u{D800}b\"; 0i32 }\n",
            "E1005",
            DiagnosticKind::Zutf8stringLiteralArgument,
        ),
    ];
    for (idx, (source, code, kind)) in cases.iter().enumerate() {
        let err = analyze(source).expect_err(&format!("case #{idx} should fail"));
        assert_eq!(err.code, *code, "case #{idx}:\n{err:?}");
        assert_eq!(err.kind, Some(kind.id()), "case #{idx}:\n{err:?}");
    }
    // `u8"..."` is rejected by ordinary type mismatch (`string` vs `utf8string`).
    let u8_err = analyze(
        "def main :: unit -> i32 = \\() -> { let _z = ZUtf8String.literal u8\"hi\"; 0i32 }\n",
    )
    .unwrap_err();
    assert_eq!(u8_err.code, "E1003");
}

#[test]
fn ref_zutf8string_is_one_pointer_and_result_copies() {
    let source = r#"
@c_layout
record Named {
    label: ref ZUtf8String,
    fallback: Maybe<ref ZUtf8String>,
}

@trusted
extern "strlen" def strlen :: ref ZUtf8String -> i64

@trusted
extern "opt_strlen" def opt_strlen :: Maybe<ref ZUtf8String> -> i64

@trusted
extern "take_named" def take_named :: ref Named -> i64

@trusted
extern "getenv" def getenv :: ref ZUtf8String -> Maybe<ZUtf8String>

extern def host_utf8 :: ref ZUtf8String -> Maybe<ref ZUtf8String> -> Maybe<ZUtf8String>

export def keep :: i32 -> i32
fn keep x = x

def main :: unit -> i32 = \() ->
    let z = ZUtf8String.literal "abc"
    let rec_val = Named { label: z, fallback: Maybe.None }
    let got = match getenv z with
        | Maybe.Some v -> (ZUtf8String.to_utf8string (ref v)).length
        | Maybe.None -> 0
    (strlen z + opt_strlen (Maybe.Some z) + opt_strlen Maybe.None + take_named (ref rec_val) + got) as i32
"#;
    let module = analyze(source).expect("valid ZUtf8String ABI declarations");
    let ir = llvm::emit_target(&module, llvm::Entry::Console, false).unwrap();
    assert!(ir.contains("declare i64 @strlen(ptr)"), "{ir}");
    assert!(ir.contains("declare i64 @opt_strlen(ptr)"), "{ir}");
    assert!(ir.contains("declare ptr @getenv(ptr)"), "{ir}");
    assert!(ir.contains("@tz.string.validate_utf8("), "{ir}");

    let header = llvm::header(&module);
    assert!(
        header.contains(
            "const char * tsuzuri_host_Main_host_utf8(const char *arg0, const char *arg1);"
        ) || header.contains(
            "const char *tsuzuri_host_Main_host_utf8(const char *arg0, const char *arg1);"
        ),
        "{header}"
    );
    assert!(header.contains("const char *label;"), "{header}");
    assert!(header.contains("const char *fallback;"), "{header}");
    assert!(
        !header.contains("ZUtf8String"),
        "opaque ZUtf8String must not emit a C struct typedef:\n{header}"
    );
}

#[test]
fn zutf8string_abi_rejections_and_wasm() {
    let cases: &[(&str, DiagnosticKind)] = &[
        (
            "extern \"f\" def f :: ZUtf8String -> i32\ndef main :: unit -> i32 = \\() -> 0i32\n",
            DiagnosticKind::UnsupportedExternSignature,
        ),
        (
            "extern \"f\" def f :: i32 -> ZUtf8String\ndef main :: unit -> i32 = \\() -> 0i32\n",
            DiagnosticKind::UnsupportedExternSignature,
        ),
        (
            "extern \"f\" def f :: Maybe<ZUtf8String> -> i32\ndef main :: unit -> i32 = \\() -> 0i32\n",
            DiagnosticKind::AbiNullablePayload,
        ),
        (
            "extern \"f\" def f :: ref mut ZUtf8String -> i32\ndef main :: unit -> i32 = \\() -> 0i32\n",
            DiagnosticKind::UnsupportedExternSignature,
        ),
        (
            "extern \"f\" def f :: (ref ZUtf8String -> i32) -> i32\ndef main :: unit -> i32 = \\() -> 0i32\n",
            DiagnosticKind::UnsupportedExternSignature,
        ),
        (
            "export def f :: ref ZUtf8String -> i32\nfn f _z = 0i32\n",
            DiagnosticKind::UnsupportedExportSignature,
        ),
        (
            "@count(1, i32)\nextern \"f\" def f :: ref ZUtf8String -> i32\ndef main :: unit -> i32 = \\() -> 0i32\n",
            DiagnosticKind::CountNotBuffer,
        ),
        (
            "@c_layout\nrecord InOnly { name: ref ZUtf8String }\nexport def f :: ref InOnly -> i32 = \\_ -> 0i32\n",
            DiagnosticKind::CLayoutInputOnly,
        ),
    ];
    for (source, kind) in cases {
        let err = analyze(source).expect_err(source);
        assert_eq!(err.code, "E1008", "{source}\n{err:?}");
        assert_eq!(err.kind, Some(kind.id()), "{source}\n{err:?}");
    }

    // Pure Tsuzuri functions work on WASM, while reached externs using ZUtf8String reject WASM.
    let wasm_pure = r#"
export def roundtrip :: i64
fn roundtrip =
    let lit = ZUtf8String.literal "Tsuzuri"
    let s = ZUtf8String.to_utf8string lit
    match ZUtf8String.of (ref s) with
    | Result.Ok z -> (ZUtf8String.to_utf8string (ref z)).length
    | Result.Error _ -> -1
"#;
    let pure_mod = analyze(wasm_pure).unwrap();
    llvm::emit_target(&pure_mod, llvm::Entry::Library, true)
        .expect("ZUtf8String module functions work on WASM");

    for wasm_bad in [
        "extern \"strlen\" def strlen :: ref ZUtf8String -> i64\nexport def run :: i64\nfn run = strlen (ZUtf8String.literal \"hi\")\n",
        "extern \"getenv\" def getenv :: i32 -> Maybe<ZUtf8String>\nexport def run :: i64\nfn run = match getenv 0i32 with | Maybe.Some _ -> 1 | Maybe.None -> 0\n",
    ] {
        let bad_mod = analyze(wasm_bad).unwrap();
        let err = llvm::emit_target(&bad_mod, llvm::Entry::Library, true).unwrap_err();
        assert_eq!(err.code, "E1008");
        assert_eq!(err.kind, Some(DiagnosticKind::AbiNativeOnly.id()));
    }
}
