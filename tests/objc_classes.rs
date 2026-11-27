//! M01: `extern class`, the built-in module `ObjC`, and the ownership of Objective-C objects.
//!
//! The expected IR shapes are written by hand from clang's output for the same constructs
//! (`clang -target arm64-apple-macosx12.0 -fobjc-arc -fno-objc-msgsend-selector-stubs -S
//! -emit-llvm`, ticket section 現状 6), not copied from this compiler.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use tsuzuri::diagnostic::{Diagnostic, DiagnosticKind};
use tsuzuri::parser::parse;
use tsuzuri::syntax::SourceKind;
use tsuzuri::target::{Platform, PlatformScope, Triple};
use tsuzuri::{analyze, analyze_modules, formatter, llvm};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_tsuzuri")
}

fn temp_project(label: &str, files: &[(&str, &str)]) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-m01-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).unwrap();
    for (path, text) in files {
        fs::write(root.join(path), text).unwrap();
    }
    root
}

fn check_err(source: &str) -> Diagnostic {
    analyze(source).expect_err(source)
}

fn rejects(source: &str, kind: DiagnosticKind) {
    let error = check_err(source);
    assert_eq!(
        error.kind,
        Some(kind.id()),
        "{source}\n{}: {}",
        error.code,
        error.message
    );
    assert_eq!(error.code, kind.code(), "{source}");
}

/// The IR of `source` for `aarch64-apple-darwin`, emitted twice to show that it is deterministic.
fn apple_ir(source: &str) -> String {
    let module = analyze(source).unwrap_or_else(|e| panic!("{source}\n{}: {}", e.code, e.message));
    let _guard = PlatformScope::enter(Platform::new(Some(Triple::Aarch64AppleDarwin)));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    ir
}

/// The classes of the E2E host, and a few functions over them.
const CLASSES: &str = "\
@trusted
extern class NSObject : ObjC.Object

@trusted
extern class TzProbe : NSObject

@trusted
extern class TzProbeChild : TzProbe

@trusted
extern class sync TzShared : NSObject

@trusted
extern \"tzobjc_make_child\" def make_child :: unit -> TzProbeChild

@trusted
extern \"tzobjc_make\" def make :: unit -> TzProbe

@trusted
extern \"tzobjc_make_shared\" def make_shared :: unit -> TzShared

@trusted
@returns_retained
extern \"tzobjc_new_probe\" def new_probe :: unit -> TzProbe

@trusted
extern \"tzobjc_maybe\" def maybe :: i32 -> Maybe<TzProbe>

@trusted
extern \"tzobjc_tag\" def tag :: ref TzProbe -> i64
";

fn with_classes(rest: &str) -> String {
    format!("{CLASSES}\n{rest}")
}

#[test]
fn parses_a_class_chain_and_types_its_values() {
    let source = "\
@trusted
extern class NSObject : ObjC.Object

/// A responder.
@trusted
extern class NSResponder : NSObject

/// A window. Its runtime name is NSWindow.
@trusted
extern \"NSWindow\" class Window : NSResponder

@trusted
extern class sync NSNumber : NSObject

extern class sync : NSObject

private extern class Hidden : ObjC.Object

@trusted @weak
extern class NSNewThing : NSObject

@trusted
extern \"window_tag\" def tag :: ref NSResponder -> i64

@trusted
@returns_retained
extern \"window_new\" def new_window :: unit -> Maybe<Window>

@trusted
@returns_not_retained
extern \"window_get\" def get_window :: unit -> Window

def use_window :: ref Window -> i64 = \\window -> tag (ObjC.upcast window)

0
";
    let program = parse(source).expect("extern class declarations must parse");
    assert_eq!(program.extern_classes.len(), 7);
    let window = &program.extern_classes[2];
    assert_eq!(window.name.text, "Window");
    assert_eq!(window.runtime_name(), "NSWindow");
    assert!(window.sync.is_none() && !window.body);
    assert!(program.extern_classes[3].sync.is_some());
    // `sync` directly before ':' is the name of the class, as `copy` is for an extern type.
    assert_eq!(program.extern_classes[4].name.text, "sync");
    assert!(program.extern_classes[4].sync.is_none());
    assert!(program.extern_classes[6].weak.is_some());
    assert_eq!(program.returns_retained.len(), 2);
    assert!(program.returns_retained[0].retained);
    assert!(!program.returns_retained[1].retained);

    let module = analyze(source).unwrap_or_else(|e| panic!("{}: {}", e.code, e.message));
    let window = &module.objc.classes["Main.Window"];
    assert_eq!(window.runtime_name, "NSWindow");
    assert_eq!(window.base.as_deref(), Some("Main.NSResponder"));
    assert_eq!(window.depth, 3);
    assert!(window.trusted && !window.sync && !window.weak);
    assert!(module.objc.classes["Main.NSNumber"].sync);
    assert!(module.objc.classes["Main.NSNewThing"].weak);
    assert!(module.objc.is_ancestor("Main.NSObject", "Main.Window"));
    assert!(module.objc.is_ancestor("ObjC.Object", "Main.Window"));
    assert!(!module.objc.is_ancestor("Main.Window", "Main.NSObject"));
    // The classes that are not `sync` make the `pinned` analysis run.
    assert!(module.thread_bound);

    let formatted = formatter::format_source("Main.tz", source, SourceKind::Code)
        .expect("the formatter handles extern class")
        .formatted;
    assert_eq!(formatted, source);
}

#[test]
fn a_program_without_objc_keeps_its_gates() {
    let module = analyze("extern type H\n0\n").unwrap();
    assert!(module.objc.is_empty() && !module.objc.has_objects());
    assert!(!module.thread_bound);
    // `ObjC.Object` alone is bound to its thread, like a class that is not `sync`.
    let module = analyze(
        "@trusted\nextern \"f\" def f :: ref ObjC.Object -> i64\ndef g :: ref ObjC.Object -> i64 = \\o -> f o\n0\n",
    )
    .unwrap();
    assert!(module.thread_bound && module.objc.has_objects());
    // `ObjC.Class` and `ObjC.Sel` are copy handles and gate nothing.
    let module = analyze(
        "@trusted\nextern \"f\" def f :: ObjC.Sel -> ObjC.Class\ndef g :: ObjC.Sel -> ObjC.Class = \\s -> f s\n0\n",
    )
    .unwrap();
    assert!(!module.thread_bound && !module.objc.has_objects());
}

#[test]
fn rejects_malformed_declarations() {
    for source in [
        // No base.
        "extern class A\n0\n",
        "extern class A :\n0\n",
        // The base is a type name.
        "extern class A : [i64]\n0\n",
        // Type parameters, a definition, type arguments of the base.
        "extern class A<'a> : ObjC.Object\n0\n",
        "extern class A = ObjC.Object\n0\n",
        "extern class A : Vec<i64>\n0\n",
        // Braces without a base.
        "extern class A {}\n0\n",
        // Two strings.
        "extern \"a\" \"b\" class A : ObjC.Object\n0\n",
        // The modifiers of an extern type.
        "extern class local A : ObjC.Object\n0\n",
        "extern class copy A : ObjC.Object\n0\n",
    ] {
        rejects(source, DiagnosticKind::ExternClassSyntax);
    }
    rejects(
        "extern \"9x\" class A : ObjC.Object\n0\n",
        DiagnosticKind::InvalidObjcClassName,
    );
    rejects(
        "extern \"NS Window\" class A : ObjC.Object\n0\n",
        DiagnosticKind::InvalidObjcClassName,
    );
    let long = format!("extern \"{}\" class A : ObjC.Object\n0\n", "a".repeat(256));
    rejects(&long, DiagnosticKind::InvalidObjcClassName);
    analyze(&format!(
        "extern \"{}\" class A : ObjC.Object\n0\n",
        "a".repeat(255)
    ))
    .expect("a runtime name of 255 bytes is accepted");
}

#[test]
fn rejects_bases_that_are_not_classes_and_bad_chains() {
    rejects(
        "record R { x: i64 }\nextern class A : R\n0\n",
        DiagnosticKind::ObjcClassBaseNotClass,
    );
    rejects(
        "extern class A : ObjC.Class\n0\n",
        DiagnosticKind::ObjcClassBaseNotClass,
    );
    rejects(
        "extern type H\nextern class A : H\n0\n",
        DiagnosticKind::ObjcClassBaseNotClass,
    );
    // An unknown base keeps the diagnostic of an unknown type.
    assert_eq!(check_err("extern class A : Missing\n0\n").code, "E1004");
    rejects(
        "extern class A : B\nextern class B : A\n0\n",
        DiagnosticKind::ObjcClassBaseCycle,
    );
    rejects(
        "extern class A : C\nextern class B : A\nextern class C : B\n0\n",
        DiagnosticKind::ObjcClassBaseCycle,
    );
    rejects(
        "extern class A : A\n0\n",
        DiagnosticKind::ObjcClassBaseCycle,
    );
    rejects(
        "extern class sync A : ObjC.Object\nextern class B : A\n0\n",
        DiagnosticKind::ObjcClassSyncBase,
    );
    analyze("extern class sync A : ObjC.Object\nextern class sync B : A\n0\n")
        .expect("a sync class may derive from a sync class");
    analyze("extern class A : ObjC.Object\nextern class sync B : A\n0\n")
        .expect("a sync class may derive from a class that is bound to its thread");

    let chain = |depth: usize| {
        let mut source = String::from("extern class C1 : ObjC.Object\n");
        for index in 2..=depth {
            source.push_str(&format!("extern class C{index} : C{}\n", index - 1));
        }
        source.push_str("0\n");
        source
    };
    let module = analyze(&chain(32)).expect("a chain of 32 classes is accepted");
    assert_eq!(module.objc.classes["Main.C32"].depth, 32);
    rejects(&chain(33), DiagnosticKind::ObjcClassLimit);
}

#[test]
fn rejects_duplicate_and_reserved_names() {
    rejects(
        "extern class A : ObjC.Object\nextern type A\n0\n",
        DiagnosticKind::DuplicateName,
    );
    rejects(
        "extern class A : ObjC.Object\nextern class A : ObjC.Object\n0\n",
        DiagnosticKind::DuplicateName,
    );
    rejects(
        "record A { x: i64 }\nextern class A : ObjC.Object\n0\n",
        DiagnosticKind::DuplicateName,
    );
    rejects(
        "@trusted pure\nextern class A : ObjC.Object\n0\n",
        DiagnosticKind::TrustedTypeProperty,
    );
    // A user module cannot take the name of the built-in module (D9).
    let error = analyze_modules(&[("Main.tz", "0\n"), ("ObjC.tz", "def one :: i64 = 1\n")])
        .expect_err("ObjC.tz is reserved");
    assert_eq!(error.code, "E1011");
    assert_eq!(error.kind, Some(DiagnosticKind::ReservedStdNamespace.id()));
}

#[test]
fn rejects_objects_outside_the_table_of_positions() {
    let owned = [
        "@trusted\nextern \"f\" def f :: TzProbe -> i64",
        "@trusted\nextern \"f\" def f :: i64 -> Maybe<TzProbe> -> i64",
        "@trusted\nextern \"f\" def f :: ObjC.Object -> unit",
    ];
    for declaration in owned {
        rejects(
            &with_classes(&format!("{declaration}\n0\n")),
            DiagnosticKind::ObjcOwnedObjectArgument,
        );
    }
    let positions = [
        // `export def`.
        "export def f :: ref TzProbe -> i64 = \\p -> tag p",
        "export def f :: ObjC.Sel -> i64 = \\_s -> 1",
        // A field of a `@c_layout` record.
        "@c_layout\nrecord R { probe: TzProbe }",
        "@c_layout\nrecord R { cls: Maybe<ObjC.Class> }",
        // `FnPtr<F>`.
        "@trusted\nextern \"f\" def f :: FnPtr<ObjC.Sel -> i64> -> i64",
        // A static callback takes no object and returns no Objective-C type.
        "@trusted\nextern \"f\" def f :: (ref TzProbe -> i64) -> i64",
        "@trusted\nextern \"f\" def f :: (i64 -> ObjC.Sel) -> i64",
        // `ref mut`.
        "@trusted\nextern \"f\" def f :: ref mut TzProbe -> i64",
        "@trusted\nextern \"f\" def f :: ref mut Maybe<TzProbe> -> i64",
        "@trusted\nextern \"f\" def f :: ref mut ObjC.Class -> i64",
        // A borrowed result, and a buffer of objects.
        "@trusted\nextern \"f\" def f :: unit -> ref TzProbe",
        "@trusted\nextern \"f\" def f :: ref [TzProbe] -> i64",
        // A data symbol reads an object, not a class or a selector.
        "@trusted\nextern \"g\" view def f :: unit -> ObjC.Class",
        "@trusted\nextern \"g\" address def f :: unit -> ObjC.Sel",
    ];
    for declaration in positions {
        rejects(
            &with_classes(&format!("{declaration}\n0\n")),
            DiagnosticKind::ObjcObjectPosition,
        );
    }
    // The positions of the table.
    analyze(&with_classes(
        "\
@trusted
extern \"a\" def a :: Maybe<ref TzProbe> -> ref ObjC.Object -> unit
@trusted
extern \"b\" def b :: ObjC.Class -> ObjC.Sel -> Maybe<ObjC.Class>
@trusted
extern \"c\" def c :: (ObjC.Class -> Maybe<ObjC.Sel> -> i64) -> ref ObjC.Sel -> ObjC.Sel
@trusted
extern \"d\" view def d :: unit -> Maybe<TzProbe>
0
",
    ))
    .unwrap_or_else(|e| panic!("{}: {}", e.code, e.message));
}

#[test]
fn rejects_misplaced_and_conflicting_ownership_attributes() {
    for source in [
        "@returns_retained\ndef f :: unit -> i64 = \\() -> 1\n0\n",
        "@returns_not_retained\nextern type H\n0\n",
        "@returns_retained\nextern class A : ObjC.Object\n0\n",
        "@returns_retained\nrecord R { x: i64 }\n0\n",
        "extern class A : ObjC.Object\n@returns_retained\nextern \"g\" view def f :: unit -> A\n0\n",
    ] {
        rejects(source, DiagnosticKind::MisplacedReturnsRetainedAttribute);
    }
    for attributes in [
        "@returns_retained\n@returns_not_retained",
        "@returns_retained @returns_retained",
        "@returns_not_retained\n@trusted\n@returns_not_retained",
    ] {
        rejects(
            &format!(
                "extern class A : ObjC.Object\n{attributes}\nextern \"f\" def f :: unit -> A\n0\n"
            ),
            DiagnosticKind::ReturnsRetainedConflict,
        );
    }
    for result in ["i64", "unit", "ObjC.Class", "Maybe<ObjC.Sel>"] {
        rejects(
            &format!("@returns_retained\nextern \"f\" def f :: unit -> {result}\n0\n"),
            DiagnosticKind::ObjcReturnsRetainedNotObject,
        );
    }
}

#[test]
fn checks_the_functions_of_the_module() {
    // The selector is a compile-time string of the right shape.
    rejects(
        &with_classes("def f :: string -> ObjC.Sel = \\s -> ObjC.sel s\n0\n"),
        DiagnosticKind::ObjcSelectorNotConstant,
    );
    rejects(
        &with_classes("def f :: i64 -> ObjC.Sel = \\n -> ObjC.sel $\"set{n}:\"\n0\n"),
        DiagnosticKind::ObjcSelectorNotConstant,
    );
    for selector in ["foo::", "", "set Title:", "setTitle:x", ":"] {
        rejects(
            &with_classes(&format!(
                "def f :: unit -> ObjC.Sel = \\() -> ObjC.sel \"{selector}\"\n0\n"
            )),
            DiagnosticKind::ObjcInvalidSelector,
        );
    }
    let sized = |bytes: usize| {
        with_classes(&format!(
            "def f :: unit -> ObjC.Sel = \\() -> ObjC.sel \"{}:\"\n0\n",
            "a".repeat(bytes - 1)
        ))
    };
    // The macOS SDK has a selector of 392 bytes; the limit is 1,024.
    analyze(&sized(392)).expect("a selector of 392 bytes");
    analyze(&sized(1024)).expect("a selector of 1,024 bytes");
    rejects(&sized(1025), DiagnosticKind::ObjcClassLimit);
    analyze(&with_classes(
        "\
const SET_TITLE: string = \"setTitle:\"
def f :: unit -> ObjC.Sel = \\() -> ObjC.sel SET_TITLE
def g :: unit -> ObjC.Sel = \\() -> ObjC.sel \"length\"
0
",
    ))
    .expect("a literal and a const string are selectors");

    // Only a direct call names a function of the module.
    for value in [
        "def f :: unit -> (ref TzProbe -> TzProbe) = \\() -> ObjC.retain",
        "def f :: unit -> (string -> ObjC.Sel) = \\() -> ObjC.sel",
        "def f :: Array<TzProbeChild> -> Array<TzProbe> = \\xs -> Array.map ObjC.upcast_owned xs",
    ] {
        rejects(
            &with_classes(&format!("{value}\n0\n")),
            DiagnosticKind::ObjcBuiltinNotCalled,
        );
    }

    // The target of an upcast is the class or an ancestor of its declared chain.
    rejects(
        &with_classes(
            "def f :: ref TzProbe -> i64 = \\p ->\n    let c: ref TzProbeChild = ObjC.upcast p\n    1\n0\n",
        ),
        DiagnosticKind::ObjcNotSuperclass,
    );
    rejects(
        &with_classes("def f :: TzProbe -> TzShared = \\p -> ObjC.upcast_owned p\n0\n"),
        DiagnosticKind::ObjcNotSuperclass,
    );
    // Without an expected type the target is unknown.
    rejects(
        &with_classes(
            "def f :: TzProbe -> i64 = \\p ->\n    let _q = ObjC.upcast_owned p\n    1\n0\n",
        ),
        DiagnosticKind::AmbiguousPolymorphicUse,
    );
    // The type arguments are Objective-C objects.
    rejects(
        &with_classes(
            "record R { x: i64 }\ndef f :: ref TzProbe -> i64 = \\p ->\n    let _r: Maybe<R> = ObjC.downcast p\n    1\n0\n",
        ),
        DiagnosticKind::ObjcTargetNotClass,
    );
    rejects(
        &with_classes("def f :: ref i64 -> i64 = \\n -> ObjC.retain n\n0\n"),
        DiagnosticKind::ObjcTargetNotClass,
    );
    rejects(
        &with_classes("def f :: ref 'a -> 'a = \\value -> ObjC.retain value\n0\n"),
        DiagnosticKind::ObjcTargetNotClass,
    );
    rejects(
        &with_classes(
            "def f :: ref ObjC.Class -> ObjC.Object = \\c -> ObjC.upcast_owned (ObjC.retain c)\n0\n",
        ),
        DiagnosticKind::ObjcTargetNotClass,
    );
    // Upcasts to the class itself, to an ancestor and to the root; a downcast to any class.
    analyze(&with_classes(
        "\
def same :: ref TzProbe -> i64 = \\p -> tag (ObjC.upcast p)
def parent :: ref TzProbeChild -> i64 = \\c -> tag (ObjC.upcast c)
def root :: TzProbeChild -> ObjC.Object = \\c -> ObjC.upcast_owned c
def down :: ref ObjC.Object -> Maybe<TzShared> = \\o -> ObjC.downcast o
def again :: ref TzProbe -> TzProbe = \\p -> ObjC.retain p
0
",
    ))
    .unwrap_or_else(|e| panic!("{}: {}", e.code, e.message));
}

#[test]
fn an_object_is_moved_and_bound_to_its_thread() {
    // A second use of an owned object is a use after the move.
    rejects(
        &with_classes(
            "\
def take :: TzProbe -> i64 = \\p -> tag (ref p)
def twice :: unit -> i64 = \\() ->
    let p = make ()
    take p + take p
0
",
        ),
        DiagnosticKind::UseAfterMove,
    );
    // No implicit conversion to a base class.
    assert_eq!(
        check_err(&with_classes(
            "def f :: ref TzProbeChild -> i64 = \\c -> tag c\n0\n"
        ))
        .code,
        "E1003"
    );
    // A function value copies its captures, and a copy would release twice.
    let captured = check_err(&with_classes(
        "\
def f :: unit -> (unit -> i64) = \\() ->
    let p = make ()
    \\() -> tag (ref p)
0
",
    ));
    assert_eq!(captured.code, "E1005", "{}", captured.message);
    let captured = check_err(&with_classes(
        "\
def f :: unit -> (unit -> i64) = \\() ->
    let s = make_shared ()
    \\() -> match ObjC.downcast (ref s) with
        | Maybe.Some (_o: TzShared) -> 1
        | Maybe.None -> 0
0
",
    ));
    assert_eq!(captured.code, "E1005", "{}", captured.message);
    // A class that is not `sync` does not cross to another tzroutine.
    let spawned = check_err(&with_classes(
        "\
tzroutine {
    let p = make ()
    let _t = tag (ref p)
}
0
",
    ));
    assert_eq!(spawned.code, "E1013", "{}", spawned.message);
    assert_eq!(spawned.kind, Some(DiagnosticKind::PinnedCodeSpawned.id()));
    let moved = check_err(&with_classes(
        "\
def f :: unit -> unit = \\() ->
    let p = make ()
    tzroutine {
        let _t = tag (ref p)
    }
0
",
    ));
    assert!(
        matches!(moved.code, "E1005" | "E1013"),
        "{}: {}",
        moved.code,
        moved.message
    );
    // Also inside a recursive union.
    let recursive = check_err(&with_classes(
        "\
union Chain = End | Link of TzProbe * Chain
def f :: Chain -> unit = \\chain ->
    tzroutine {
        let _c = chain
    }
0
",
    ));
    assert!(
        matches!(recursive.code, "E1005" | "E1013"),
        "{}: {}",
        recursive.code,
        recursive.message
    );
    // A `sync` class moves into a tzroutine body, which runs once.
    analyze(&with_classes(
        "\
def f :: unit -> unit = \\() ->
    let s = make_shared ()
    tzroutine {
        let _moved = s
    }
0
",
    ))
    .unwrap_or_else(|e| panic!("{}: {}", e.code, e.message));
    // Objects are fields, payloads and elements like any other moved value.
    analyze(&with_classes(
        "\
record Pair { first: TzProbe, second: Maybe<TzProbe> }
def f :: unit -> i64 = \\() ->
    let pair = Pair { first: make (), second: maybe 1i32 }
    let probes = [make (), new_probe ()]
    tag (ref pair.first) + tag (ref probes[0])
0
",
    ))
    .unwrap_or_else(|e| panic!("{}: {}", e.code, e.message));
}

#[test]
fn lowers_like_the_clang_reference() {
    let ir = apple_ir(&with_classes(
        "\
@trusted
extern \"tzobjc_sel_is\" def sel_is :: ObjC.Sel -> ref utf8string -> i32

export def casts :: i64 = \\() ->
    let child = make_child ()
    let base: TzProbe = ObjC.upcast_owned (ObjC.retain (ref child))
    let again: Maybe<TzProbeChild> = ObjC.downcast (ref base)
    let any: Maybe<ObjC.Object> = ObjC.downcast (ref base)
    let found = match again with
        | Maybe.Some c -> tag (ObjC.upcast (ref c))
        | Maybe.None -> 0
    let other = match any with
        | Maybe.Some _ -> 1
        | Maybe.None -> 0
    found + other + tag (ObjC.upcast (ref child))

export def selectors :: i32 = \\() ->
    let a = sel_is (ObjC.sel \"description\") (ref u8\"description\")
    let b = sel_is (ObjC.sel \"setTitle:\") (ref u8\"setTitle:\")
    let c = sel_is (ObjC.sel \"description\") (ref u8\"description\")
    a + b + c

export def owned :: i64 = \\() ->
    let probe = new_probe ()
    tag (ref probe)
",
    ));
    // A classref, as clang writes it for `[NSWindow class]`: only the class of the downcast.
    assert!(
        ir.contains("@\"OBJC_CLASS_$_TzProbeChild\" = external global [0 x i8]\n"),
        "{ir}"
    );
    assert!(
        ir.contains(
            "@tz.objc.classref.TzProbeChild = internal global ptr @\"OBJC_CLASS_$_TzProbeChild\", section \"__DATA,__objc_classrefs,regular,no_dead_strip\", align 8\n"
        ),
        "{ir}"
    );
    // Its definition, its entry in `@llvm.compiler.used`, and the one load of the downcast.
    assert_eq!(ir.matches("@tz.objc.classref.").count(), 3, "{ir}");
    // One selref for each selector, numbered by first use.
    assert!(
        ir.contains(
            "@tz.objc.methname.0 = private unnamed_addr constant [12 x i8] c\"description\\00\", section \"__TEXT,__objc_methname,cstring_literals\", align 1\n"
        ),
        "{ir}"
    );
    assert!(
        ir.contains(
            "@tz.objc.selref.0 = internal externally_initialized global ptr @tz.objc.methname.0, section \"__DATA,__objc_selrefs,literal_pointers,no_dead_strip\", align 8\n"
        ),
        "{ir}"
    );
    assert!(
        ir.contains(
            "@tz.objc.methname.1 = private unnamed_addr constant [10 x i8] c\"setTitle:\\00\""
        ),
        "{ir}"
    );
    assert!(!ir.contains("@tz.objc.selref.2"), "{ir}");
    assert!(
        ir.contains(
            "@llvm.compiler.used = appending global [5 x ptr] [ptr @tz.objc.classref.TzProbeChild, ptr @tz.objc.methname.0, ptr @tz.objc.selref.0, ptr @tz.objc.methname.1, ptr @tz.objc.selref.1], section \"llvm.metadata\"\n"
        ),
        "{ir}"
    );
    // Only the load of a selref is invariant.
    let invariant: Vec<&str> = ir
        .lines()
        .filter(|line| line.contains("!invariant.load") && line.contains("tz.objc."))
        .collect();
    assert_eq!(invariant.len(), 3, "{ir}");
    assert!(
        invariant
            .iter()
            .all(|line| line.contains("load ptr, ptr @tz.objc.selref.")),
        "{invariant:?}"
    );
    assert!(
        ir.lines().any(|line| line
            .trim_end()
            .ends_with("load ptr, ptr @tz.objc.classref.TzProbeChild, align 8")),
        "{ir}"
    );
    // The downcast and the plain retain and release.
    assert!(ir.contains("call i1 @objc_opt_isKindOfClass(ptr %"), "{ir}");
    assert!(ir.contains("call ptr @objc_retain(ptr %"), "{ir}");
    assert!(ir.contains("call void @objc_release(ptr %"), "{ir}");
    // A +0 result carries the bundle; `@returns_retained` carries none.
    assert!(
        ir.contains(
            "call ptr @tzobjc_make_child() [ \"clang.arc.attachedcall\"(ptr @llvm.objc.retainAutoreleasedReturnValue) ]\n"
        ),
        "{ir}"
    );
    assert!(ir.contains("call ptr @tzobjc_new_probe()\n"), "{ir}");
    assert!(
        !ir.contains("@objc_retainAutoreleasedReturnValue("),
        "the bundle needs no plain call:\n{ir}"
    );
    // One declaration for each symbol.
    for declaration in [
        "declare ptr @objc_retain(ptr)",
        "declare void @objc_release(ptr)",
        "declare i1 @objc_opt_isKindOfClass(ptr, ptr)",
        "declare ptr @llvm.objc.retainAutoreleasedReturnValue(ptr)",
    ] {
        assert_eq!(
            ir.lines().filter(|line| *line == declaration).count(),
            1,
            "{declaration}\n{ir}"
        );
    }
    assert!(llvm::duplicate_declarations(&ir).is_empty());
    // The six flags of an Objective-C image, on one line.
    for flag in [
        "!{i32 1, !\"Objective-C Version\", i32 2}",
        "!{i32 1, !\"Objective-C Image Info Version\", i32 0}",
        "!{i32 1, !\"Objective-C Image Info Section\", !\"__DATA,__objc_imageinfo,regular,no_dead_strip\"}",
        "!{i32 1, !\"Objective-C Garbage Collection\", i8 0}",
        "!{i32 1, !\"Objective-C Class Properties\", i32 64}",
        "!{i32 1, !\"Objective-C Enforce ClassRO Pointer Signing\", i8 0}",
    ] {
        assert_eq!(ir.matches(flag).count(), 1, "{flag}\n{ir}");
    }
    let flags: Vec<&str> = ir
        .lines()
        .filter(|line| line.starts_with("!llvm.module.flags"))
        .collect();
    assert_eq!(flags.len(), 1, "{ir}");
    assert_eq!(flags[0].matches('!').count(), 8, "{}", flags[0]);
    assert!(llvm::uses_objc(&ir));
}

#[test]
fn an_object_free_program_emits_nothing_of_objc() {
    // Declarations alone, and a function that only passes a selector on.
    let ir = apple_ir(
        "\
@trusted
extern class NSObject : ObjC.Object
@trusted
extern \"use\" def use_sel :: ObjC.Sel -> i64
export def f :: i64 -> i64 = \\x -> x + 1
",
    );
    assert!(!llvm::uses_objc(&ir), "{ir}");
    for absent in ["objc", "OBJC", "llvm.compiler.used", "llvm.module.flags"] {
        assert!(!ir.contains(absent), "{absent}\n{ir}");
    }
}

#[test]
fn shares_a_declaration_with_the_program() {
    // The program declares `objc_release` itself, with another C type: the IR keeps that one
    // line, and the compiler's call passes a pointer.
    let ir = apple_ir(&with_classes(
        "\
@trusted
extern \"objc_release\" def release :: i64 -> unit

export def f :: i64 = \\() ->
    let probe = make ()
    release 0
    tag (ref probe)
",
    ));
    let declarations: Vec<&str> = ir
        .lines()
        .filter(|line| line.starts_with("declare ") && line.contains("@objc_release("))
        .collect();
    assert_eq!(
        declarations,
        vec!["declare void @objc_release(i64)"],
        "{ir}"
    );
    assert!(ir.contains("call void @objc_release(ptr %"), "{ir}");
    assert!(llvm::duplicate_declarations(&ir).is_empty());

    // One symbol declared with an `i64` result and with a class result (W14): the canonical C
    // result is `i64`, which cannot carry the bundle, so the class result is retained by a plain
    // call after its `inttoptr`.
    let ir = apple_ir(&with_classes(
        "\
@trusted
extern \"tzobjc_raw\" def raw :: unit -> i64
@trusted
extern \"tzobjc_raw\" def typed :: unit -> TzProbe

export def f :: i64 = \\() ->
    let probe = typed ()
    raw () + tag (ref probe)
",
    ));
    assert!(ir.contains("declare i64 @tzobjc_raw()"), "{ir}");
    assert!(
        !ir.lines()
            .any(|line| line.contains("@tzobjc_raw()") && line.contains("clang.arc.attachedcall")),
        "{ir}"
    );
    assert!(
        ir.contains("call ptr @objc_retainAutoreleasedReturnValue(ptr %"),
        "{ir}"
    );
    assert_eq!(
        ir.lines()
            .filter(|line| *line == "declare ptr @objc_retainAutoreleasedReturnValue(ptr)")
            .count(),
        1,
        "{ir}"
    );
    assert!(
        !ir.contains("declare ptr @llvm.objc.retainAutoreleasedReturnValue(ptr)"),
        "{ir}"
    );
}

#[test]
fn a_weak_class_checks_each_read() {
    let ir = apple_ir(
        "\
@trusted
extern class NSObject : ObjC.Object
@trusted @weak
extern class NSNewThing : NSObject
@trusted
extern \"make\" def make :: unit -> NSObject

export def f :: i64 = \\() ->
    let object = make ()
    let first: Maybe<NSNewThing> = ObjC.downcast (ref object)
    let second: Maybe<NSNewThing> = ObjC.downcast (ref object)
    match (first, second) with
    | (Maybe.Some _, Maybe.Some _) -> 2
    | _ -> 0
",
    );
    assert!(
        ir.contains("@\"OBJC_CLASS_$_NSNewThing\" = extern_weak global [0 x i8]\n"),
        "{ir}"
    );
    // Every read of the class reference is followed by its check (the two downcasts share one
    // instance of `ObjC.downcast`, so the IR may hold one read for both).
    let reads = ir
        .matches("load ptr, ptr @tz.objc.classref.NSNewThing, align 8")
        .count();
    assert!(reads >= 1, "{ir}");
    assert_eq!(
        ir.matches("call void @tz.weak.missing(ptr @tz.weak.name.OBJC_CLASS_$_NSNewThing, i64 23)")
            .count(),
        reads,
        "{ir}"
    );
    assert_eq!(
        ir.matches("@tz.weak.name.OBJC_CLASS_$_NSNewThing =")
            .count(),
        1,
        "{ir}"
    );
}

#[test]
fn reads_an_object_from_a_data_symbol_with_a_retain() {
    let ir = apple_ir(&with_classes(
        "\
@trusted
extern \"tzobjc_global\" view def global :: unit -> Maybe<TzProbe>
@trusted
extern \"tzobjc_other\" view def global_now :: unit -> TzProbe

export def f :: i64 = \\() ->
    let first = match global () with
        | Maybe.Some probe -> tag (ref probe)
        | Maybe.None -> 0
    let probe = global_now ()
    first + tag (ref probe)
",
    ));
    assert!(
        ir.contains("@tzobjc_global = external global [0 x i8]"),
        "{ir}"
    );
    assert!(
        ir.contains("@tzobjc_other = external global [0 x i8]"),
        "{ir}"
    );
    assert_eq!(
        ir.matches("load ptr, ptr @tzobjc_global, align 8").count(),
        1,
        "{ir}"
    );
    assert_eq!(
        ir.matches("load ptr, ptr @tzobjc_other, align 8").count(),
        1,
        "{ir}"
    );
    assert!(
        ir.matches("call ptr @objc_retain(ptr %").count() >= 2,
        "{ir}"
    );
    assert!(llvm::uses_objc(&ir));
}

#[test]
fn the_flags_join_the_debug_information() {
    let root = temp_project(
        "debug",
        &[(
            "Main.tz",
            &with_classes(
                "export def f :: i64 = \\() ->\n    let probe = make ()\n    tag (ref probe)\n",
            ),
        )],
    );
    let output = root.join("out.ll");
    let status = Command::new(bin())
        .arg("build")
        .arg(&root)
        .args([
            "--emit",
            "llvm",
            "-g",
            "--target",
            "aarch64-apple-darwin",
            "-o",
        ])
        .arg(&output)
        .env("TSUZURI_CACHE_DIR", "")
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    let ir = fs::read_to_string(&output).unwrap();
    let flags: Vec<&str> = ir
        .lines()
        .filter(|line| line.starts_with("!llvm.module.flags"))
        .collect();
    assert_eq!(flags.len(), 1, "{ir}");
    // Two flags of the debug information and six of Objective-C.
    assert_eq!(flags[0].matches('!').count(), 10, "{}", flags[0]);
    assert!(ir.contains("!\"Dwarf Version\""), "{ir}");
    assert!(ir.contains("!\"Objective-C Version\""), "{ir}");
    fs::remove_dir_all(root).ok();
}

#[test]
fn a_blocking_wrapper_keeps_the_bundle_on_its_call() {
    // TZ08: a program that starts a tzroutine wraps its extern calls. The bundle is part of the
    // call, so it stays next to it between the enter and the exit.
    let module = analyze(&with_classes(
        "\
def work :: unit -> i64 = \\() ->
    let shared = make_shared ()
    match ObjC.downcast (ref shared) with
    | Maybe.Some (_o: TzShared) -> 1
    | Maybe.None -> 0

def main :: unit -> i32 = \\() ->
    tzroutine {
        let _n = work ()
    }
    0
",
    ))
    .unwrap_or_else(|e| panic!("{}: {}", e.code, e.message));
    let _guard = PlatformScope::enter(Platform::new(Some(Triple::Aarch64AppleDarwin)));
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    let lines: Vec<&str> = ir.lines().collect();
    let call = lines
        .iter()
        .position(|line| line.contains("call ptr @tzobjc_make_shared()"))
        .unwrap_or_else(|| panic!("{ir}"));
    assert!(
        lines[call].trim_end().ends_with(
            "[ \"clang.arc.attachedcall\"(ptr @llvm.objc.retainAutoreleasedReturnValue) ]"
        ),
        "{}",
        lines[call]
    );
    assert!(
        lines[call - 1].contains("@tsuzuri_tz_blocking_enter"),
        "{}\n{}",
        lines[call - 1],
        lines[call]
    );
    assert!(
        lines[call + 1].contains("@tsuzuri_tz_blocking_exit"),
        "{}\n{}",
        lines[call],
        lines[call + 1]
    );
}

/// `--emit bindings-js` refuses what the WebAssembly build of the same module refuses: glue for a
/// module that cannot be built would load nothing.
#[test]
fn javascript_bindings_refuse_a_reached_use_of_objc() {
    let root = temp_project(
        "bindings",
        &[(
            "Main.tz",
            "\
@trusted
extern \"use_sel\" def use_sel :: ObjC.Sel -> i64
export def f :: i64 = \\() -> use_sel (ObjC.sel \"length\")
",
        )],
    );
    let build = |emit: &str, output: &str| {
        Command::new(bin())
            .arg("build")
            .arg(&root)
            .args(["--target", "wasm32", "--emit", emit, "-o"])
            .arg(root.join(output))
            .env("TSUZURI_CACHE_DIR", "")
            .output()
            .unwrap()
    };
    for (emit, output) in [("bindings-js", "out.mjs"), ("llvm", "out.ll")] {
        let result = build(emit, output);
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(!result.status.success(), "{emit}: {stderr}");
        assert!(stderr.contains("E1008"), "{emit}: {stderr}");
        assert!(
            stderr.contains("WebAssembly targets do not have"),
            "{emit}: {stderr}"
        );
        assert!(!root.join(output).exists(), "{emit}");
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn only_an_apple_target_reaches_objc() {
    let reached = with_classes(
        "export def f :: i64 = \\() ->\n    let probe = make ()\n    tag (ref probe)\n",
    );
    let selector_only = "\
@trusted
extern class NSObject : ObjC.Object
@trusted
extern \"use_sel\" def use_sel :: ObjC.Sel -> i64
export def f :: i64 = \\() -> use_sel (ObjC.sel \"length\")
";
    let declared = "\
@trusted
extern class NSObject : ObjC.Object
@trusted
extern \"make\" def make :: unit -> NSObject
export def f :: i64 -> i64 = \\x -> x + 1
";
    for source in [reached.as_str(), selector_only] {
        let module = analyze(source).unwrap();
        let wasm = llvm::emit_target(&module, llvm::Entry::Library, true).unwrap_err();
        assert_eq!(
            wasm.kind,
            Some(DiagnosticKind::AbiNativeOnly.id()),
            "{source}"
        );
        assert_eq!(wasm.code, "E1008");
        for triple in [
            Triple::X86_64LinuxGnu,
            Triple::Aarch64LinuxGnu,
            Triple::X86_64WindowsMsvc,
        ] {
            let _guard = PlatformScope::enter(Platform::new(Some(triple)));
            let other = llvm::emit(&module, llvm::Entry::Library).unwrap_err();
            assert_eq!(
                other.kind,
                Some(DiagnosticKind::AbiAppleOnly.id()),
                "{source}"
            );
            assert_eq!(other.code, "E1008");
        }
    }
    // Declarations that nothing reaches change no output on any target.
    let module = analyze(declared).unwrap();
    let plain = analyze("export def f :: i64 -> i64 = \\x -> x + 1\n").unwrap();
    assert_eq!(
        llvm::emit_target(&module, llvm::Entry::Library, true).unwrap(),
        llvm::emit_target(&plain, llvm::Entry::Library, true).unwrap()
    );
    for triple in [Triple::X86_64LinuxGnu, Triple::Aarch64AppleDarwin] {
        let _guard = PlatformScope::enter(Platform::new(Some(triple)));
        assert_eq!(
            llvm::emit(&module, llvm::Entry::Library).unwrap(),
            llvm::emit(&plain, llvm::Entry::Library).unwrap()
        );
    }
}
