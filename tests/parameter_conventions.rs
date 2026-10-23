use std::sync::atomic::{AtomicUsize, Ordering};
use tsuzuri::check::CheckedModule;
use tsuzuri::diagnostic::Diagnostic;
use tsuzuri::driver::Project;
use tsuzuri::syntax::SourceKind;
use tsuzuri::{formatter, llvm};

fn check(edition: &str, source: &str) -> Result<CheckedModule, Diagnostic> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/scratch/parameter-conventions")
        .join(format!(
            "{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Main.tz"), source).unwrap();
    std::fs::write(
        root.join("Tsuzuri.toml"),
        format!(
            "[package]\nname = \"conventions\"\nversion = \"0.1.0\"\nedition = \"{edition}\"\n"
        ),
    )
    .unwrap();
    let result = Project::load(&root).unwrap().analyze();
    std::fs::remove_dir_all(root).unwrap();
    result
}

fn ir(edition: &str, source: &str) -> String {
    let module = check(edition, source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    ir
}

#[test]
fn only_2027_borrows_unmarked_owned_data() {
    let source = "def size :: string -> i64 = \\s -> s.length\n\
        export def answer :: i64 = { let text = \"hello\"; size text + text.length }\n";
    assert_eq!(check("2026", source).unwrap_err().code, "E1012");
    assert_eq!(tsuzuri::analyze(source).unwrap_err().code, "E1012");
    assert_eq!(
        ir("2027", source),
        ir("2027", &source.replace(":: string", ":: ref string"))
    );
}

#[test]
fn borrowed_arrays_match_explicit_references_without_copying() {
    let source = "def first :: [i64] -> i64 = \\xs -> xs[0]\n\
        export def answer :: i64 = { let xs = [10, 20]; first xs + xs[1] }\n";
    let borrowed = ir("2027", source);
    assert_eq!(
        borrowed,
        ir("2027", &source.replace(":: [i64]", ":: ref [i64]"))
    );
    let answer = borrowed
        .split("\n\n")
        .find(|block| block.starts_with("define ") && block.contains("@tz.fn.Main.answer("))
        .unwrap();
    assert!(!answer.contains("call ptr @tz.alloc"), "{answer}");
    assert_ne!(borrowed, ir("2026", source));
}

#[test]
fn own_is_additive_and_keeps_the_2026_value_convention() {
    let source = "def take :: own string -> string = \\s -> s\n\
        export def answer :: i64 = (take \"hello\").length\n";
    assert_eq!(ir("2027", source), ir("2026", source));
    assert_eq!(ir("2026", source), ir("2026", &source.replace("own ", "")));
    let moved = "def take :: own string -> i64 = \\s -> s.length\n\
        def bad :: unit -> i64 = \\() -> { let s = \"x\"; take s + s.length }\n";
    assert_eq!(check("2027", moved).unwrap_err().code, "E1012");
}

#[test]
fn own_is_only_a_whole_def_parameter_and_is_not_reserved() {
    for source in [
        "def bad :: i64 -> own string = \\n -> \"x\"\n",
        "def bad :: unit -> i64 = \\() -> { let s: own string = \"x\"; s.length }\n",
        "record Bad { value: own string }\n",
        "def bad :: (own string -> i64) -> i64 = \\f -> 0\n",
        "def bad :: own own string -> i64 = \\s -> s.length\n",
    ] {
        for edition in ["2026", "2027"] {
            assert_eq!(
                check(edition, source).unwrap_err().code,
                "E1005",
                "{source}"
            );
        }
    }
    let source = "def own :: i64 -> i64 = \\own -> own + 1\n";
    assert_eq!(ir("2026", source), ir("2027", source));
}

#[test]
fn scalar_and_generic_function_conventions_remain_values_in_phase_one() {
    let source = "record Point { x: i64, y: i64 }\n\
        def get :: Point -> i64 = \\p -> p.x\n\
        def identity :: 'a -> 'a = \\x -> x\n\
        def apply :: ('a -> 'b) -> 'a -> 'b = \\f x -> f x\n\
        export def answer :: i64 -> i64 = \\n -> apply identity (get (Point { x: n, y: 2 }))\n";
    assert_eq!(ir("2026", source), ir("2027", source));
    check("2027", "def length :: ['a] -> i64 = \\xs -> xs.length\n").unwrap();
}

#[test]
fn fixed_signature_contracts_are_not_complemented() {
    check(
        "2027",
        "const def size :: string -> i64 = \\s -> s.length\n\
         const Eight: i64 = size \"abcdefgh\"\n\
         def main :: [string] -> i32 = \\args -> args.length as i32\n",
    )
    .unwrap();
    assert_eq!(
        check(
            "2027",
            "const def size :: ref string -> i64 = \\s -> s.length\n"
        )
        .unwrap_err()
        .code,
        "E1026"
    );
    assert_eq!(
        check("2027", "extern def host_len :: string -> i64\n")
            .unwrap_err()
            .code,
        "E1008"
    );
    check("2027", "extern def host_len :: ref string -> i64\n").unwrap();
}

#[test]
fn default_read_diagnostics_explain_ownership_but_explicit_borrows_do_not() {
    for (source, code) in [
        ("def bad :: string -> string = \\s -> s\n", "E1003"),
        (
            "record Named { text: string, n: i64 }\ndef bad :: Named -> string = \\p -> p.text\n",
            "E1012",
        ),
        (
            "record Named { text: string, n: i64 }\ndef bad :: Named -> Named = \\p -> { p with n = 1 }\n",
            "E1012",
        ),
    ] {
        let error = check("2027", source).unwrap_err();
        assert_eq!(error.code, code, "{error:?}");
        assert!(error.message.contains("read-only by default"), "{error:?}");
        assert!(error.message.contains("write 'own "), "{error:?}");
        let explicit = source
            .replace(":: string", ":: ref string")
            .replace(":: Named", ":: ref Named");
        let error = check("2027", &explicit).unwrap_err();
        assert_eq!(error.code, code);
        assert!(!error.message.contains("read-only by default"), "{error:?}");
        check("2026", source).unwrap();
    }
}

#[test]
fn own_survives_formatter_and_documentation_round_trips() {
    let source = "def take::own [|string|]->i64=\\xs->xs.length\n";
    let formatted = formatter::format_source("Main.tz", source, SourceKind::Code)
        .unwrap()
        .formatted;
    assert!(formatted.contains("own [|string|]"), "{formatted}");
    let second = formatter::format_source("Main.tz", &formatted, SourceKind::Code)
        .unwrap()
        .formatted;
    assert_eq!(formatted, second);
    let program = tsuzuri::parser::parse(&formatted).unwrap();
    assert!(tsuzuri::docgen::render_module("Main", &program).contains("own [|string|]"));
    check("2027", &formatted).unwrap();
}
