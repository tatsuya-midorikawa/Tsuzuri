//! M02: members of `extern class` and `extern category`, typed message sends, the Cocoa
//! families, `Self`, `ObjC.literal`, and `@c_layout` records by value in a message.
//!
//! The expected call types are written by hand from clang's output for the same messages
//! (`clang -target arm64-apple-macosx12.0 -fobjc-arc -fno-objc-msgsend-selector-stubs -S
//! -emit-llvm`, ticket section 現状 5 and 7); `tests/objc_send.mjs` compares every call of its
//! fixture with clang itself on a Mac.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use tsuzuri::check::{CheckedModule, TypedExprKind};
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
        "tsuzuri-m02-{label}-{}-{}",
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

fn accepts(source: &str) -> CheckedModule {
    analyze(source).unwrap_or_else(|e| panic!("{source}\n{}: {}", e.code, e.message))
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

fn apple_ir(source: &str) -> String {
    let module = accepts(source);
    let _guard = PlatformScope::enter(Platform::new(Some(Triple::Aarch64AppleDarwin)));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    ir
}

/// The `objc_msgSend` calls of `ir`, without the names of their values.
fn sends(ir: &str) -> Vec<String> {
    ir.lines()
        .filter(|line| line.contains("@objc_msgSend(") && !line.starts_with("declare"))
        .map(|line| {
            let call = &line[line.find("call ").expect("a call")..];
            let mut text = String::new();
            let mut rest = call;
            while let Some(at) = rest.find('%') {
                text.push_str(&rest[..at]);
                text.push('%');
                rest = rest[at + 1..].trim_start_matches(|c: char| {
                    c.is_ascii_alphanumeric() || c == '.' || c == '_'
                });
            }
            text.push_str(rest);
            text
        })
        .collect()
}

/// The signature of the function that stands for `owner.member`, as the checker resolved it.
fn member_type(module: &CheckedModule, owner: &str, member: &str) -> String {
    let function = module
        .functions
        .iter()
        .find(|function| {
            matches!(
                &function.body.kind,
                TypedExprKind::HostCall(import, _)
                    if import.objc.as_ref().is_some_and(|send| send.display == format!("{owner}.{member}"))
            )
        })
        .unwrap_or_else(|| panic!("no function for {owner}.{member}"));
    let types = module.types();
    let mut text: Vec<String> = function
        .signature
        .parameters
        .iter()
        .map(|parameter| parameter.display(&types))
        .collect();
    text.push(function.signature.result.display(&types));
    text.join(" -> ")
}

const COCOA: &str = "\
@c_layout
record NSPoint { x: f64, y: f64 }

@c_layout
record NSSize { width: f64, height: f64 }

@c_layout
record NSRect { origin: NSPoint, size: NSSize }

@c_layout
record NSRange { location: i64u, length: i64u }

@c_layout
record Transform { a: f64, b: f64, c: f64, d: f64, tx: f64, ty: f64 }

@trusted
extern class NSObject : ObjC.Object {
    class alloc :: unit -> Self
    init :: unit -> Maybe<Self>
    class \"class\" class_object :: unit -> ObjC.Class
    \"respondsToSelector:\" responds :: ObjC.Sel -> bool
}

@trusted
extern class NSString : NSObject {
    \"initWithBytes:length:encoding:\" init_with_bytes :: ref [ubyte] -> i64u -> Maybe<Self>
    length :: unit -> i64u
    \"isEqualToString:\" is_equal_to_string :: ref NSString -> bool
    \"mutableCopy\" mutable_copy :: unit -> NSMutableString
}

@trusted
extern class NSMutableString : NSString

@trusted
extern class NSWindow : NSObject {
    \"initWithContentRect:styleMask:backing:defer:\" init_with_content_rect :: NSRect -> i64u -> i64u -> bool -> Maybe<Self>
    \"setTitle:\" set_title :: ref NSString -> unit
    title :: unit -> NSString
    frame :: unit -> NSRect
    \"isVisible\" is_visible :: unit -> bool
    \"backingScaleFactor\" backing_scale_factor :: unit -> f64
    \"alphaValue\" alpha_value :: unit -> f32
    \"selectedRange\" selected_range :: unit -> NSRange
    \"transformStruct\" transform_struct :: unit -> Transform
    \"setFrameOrigin:\" set_frame_origin :: NSPoint -> unit
    \"setTransformStruct:\" set_transform_struct :: Transform -> unit
    \"getFlag:\" get_flag :: ref mut bool -> unit
    \"takeChar:short:uchar:ushort:flag:f:\" take :: i8 -> i16 -> i8u -> i16u -> bool -> f32 -> i64
    \"narrow:\" narrow :: i8 -> i8
    \"ushorter:\" ushorter :: i16u -> i16u
}

@trusted
extern category Geometry : NSWindow {
    \"convertPointToScreen:\" convert_point_to_screen :: NSPoint -> NSPoint
    class \"windowNumberAtPoint:belowWindowWithWindowNumber:\" number_at_point :: NSPoint -> i64 -> i64
}
";

fn with_cocoa(rest: &str) -> String {
    format!("{COCOA}\n{rest}")
}

#[test]
fn parses_members_and_categories() {
    let source = with_cocoa("0\n");
    let program = parse(&source).expect("members and categories parse");
    assert_eq!(program.extern_classes.len(), 4);
    assert_eq!(program.extern_categories.len(), 1);
    let object = &program.extern_classes[0];
    assert!(object.body && object.members.len() == 4);
    assert!(object.members[0].class_method.is_some());
    assert_eq!(object.members[0].selector(), "alloc");
    assert_eq!(object.members[2].name.text, "class_object");
    assert_eq!(object.members[2].selector(), "class");
    assert!(!program.extern_classes[2].body);
    let geometry = &program.extern_categories[0];
    assert_eq!(geometry.name.text, "Geometry");
    assert_eq!(geometry.members.len(), 2);

    let module = accepts(&with_cocoa(
        "\
def titled :: ref NSWindow -> ref NSString -> unit = \\window title -> NSWindow.set_title window title
def text :: ref [ubyte] -> Maybe<NSString> = \\bytes -> NSString.init_with_bytes (NSString.alloc ()) bytes 4i64u
def made :: unit -> Maybe<NSMutableString> = \\() -> NSMutableString.init (NSMutableString.alloc ())
0
",
    ));
    assert_eq!(
        member_type(&module, "NSWindow", "set_title"),
        "ref Main.NSWindow -> ref Main.NSString -> unit"
    );
    // `init` takes its receiver; `Self` is the class that the call names.
    assert_eq!(
        member_type(&module, "NSString", "init_with_bytes"),
        "Main.NSString -> ref [i8u] -> i64u -> Maybe<Main.NSString>"
    );
    assert_eq!(
        member_type(&module, "NSMutableString", "alloc"),
        "unit -> Main.NSMutableString"
    );
    assert_eq!(
        member_type(&module, "NSMutableString", "init"),
        "Main.NSMutableString -> Maybe<Main.NSMutableString>"
    );
    assert_eq!(
        member_type(&module, "Geometry", "convert_point_to_screen"),
        "ref Main.NSWindow -> Main.NSPoint -> Main.NSPoint"
    );
    assert_eq!(
        member_type(&module, "Geometry", "number_at_point"),
        "Main.NSPoint -> i64 -> i64"
    );

    let formatted = formatter::format_source("Main.tz", &source, SourceKind::Code)
        .expect("the formatter handles members and categories")
        .formatted;
    assert_eq!(formatted, source);
}

#[test]
fn rejects_malformed_members_and_categories() {
    let member = |text: &str| format!("extern class A : ObjC.Object {{\n    {text}\n}}\n0\n");
    for text in [
        // No `::`, type parameters, a body, a type that is no function.
        "length",
        "length : unit -> i64",
        "length<'a> :: unit -> 'a",
        "length :: unit -> i64 = 1",
        "length :: i64",
        // Another attribute.
        "@cpu [\"avx2\"]\n    length :: unit -> i64",
    ] {
        rejects(&member(text), DiagnosticKind::ExternClassMemberSyntax);
    }
    for text in [
        "\"take:\" take :: Self -> unit",
        "\"take:\" take :: ref Self -> unit",
        "items :: unit -> Vec<Self>",
        "pair :: unit -> (Self * i64)",
        "\"other:\" other :: Maybe<Self> -> unit",
    ] {
        rejects(&member(text), DiagnosticKind::ObjcSelfPosition);
    }
    rejects(
        &member("@count(1, none)\n    \"bytes:\" bytes :: ref [ubyte] -> unit"),
        DiagnosticKind::MisplacedCountAttribute,
    );
    rejects(
        &member("@trusted\n    length :: unit -> i64"),
        DiagnosticKind::TrustedWrongTarget,
    );
    rejects(
        &member("@returns_retained\n    @returns_not_retained\n    twin :: unit -> A"),
        DiagnosticKind::ReturnsRetainedConflict,
    );
    rejects(
        &member("@returns_retained\n    count :: unit -> i64"),
        DiagnosticKind::ObjcReturnsRetainedNotObject,
    );
    for selector in ["foo::", "set title:", "", "a:b"] {
        rejects(
            &member(&format!("\"{selector}\" named :: i64 -> i64 -> unit")),
            DiagnosticKind::ObjcInvalidSelector,
        );
    }
    let long = format!("\"{}:\" named :: i64 -> unit", "a".repeat(1024));
    rejects(&member(&long), DiagnosticKind::ObjcClassLimit);
    // Outside a member `Self` is an unknown type, as before.
    assert_eq!(
        check_err("def f :: Self -> i64 = \\_s -> 1\n0\n").code,
        "E1004"
    );

    for source in [
        // A link name, no class, `sync`, no braces.
        "extern class A : ObjC.Object\nextern \"Extras\" category E : A {}\n0\n",
        "extern class A : ObjC.Object\nextern category E {}\n0\n",
        "extern class A : ObjC.Object\nextern category sync E : A {}\n0\n",
        "extern class A : ObjC.Object\nextern category E : A\n0\n",
    ] {
        rejects(source, DiagnosticKind::ExternCategorySyntax);
    }
    rejects(
        "record R { x: i64 }\nextern category E : R {}\n0\n",
        DiagnosticKind::ObjcClassBaseNotClass,
    );
    rejects(
        "extern class A : ObjC.Object\nextern category E : A {}\nextern category E : A {}\n0\n",
        DiagnosticKind::DuplicateName,
    );
    rejects(
        "extern class A : ObjC.Object\nextern category A : A {}\n0\n",
        DiagnosticKind::DuplicateName,
    );
}

#[test]
fn counts_the_colons_of_a_selector() {
    let member = |text: &str| format!("extern class A : ObjC.Object {{\n    {text}\n}}\n0\n");
    // More arguments than colons would be variadic.
    rejects(
        &member("\"setTitle:\" set_title :: ref A -> i64 -> unit"),
        DiagnosticKind::ObjcVariadicMethod,
    );
    // Fewer, and a member with arguments that leaves the selector out.
    rejects(
        &member("\"a:b:\" two :: i64 -> unit"),
        DiagnosticKind::ObjcSelectorArity,
    );
    rejects(
        &member("set_title :: ref A -> unit"),
        DiagnosticKind::ObjcSelectorArity,
    );
    rejects(
        &member("\"length\" length :: i64 -> i64"),
        DiagnosticKind::ObjcVariadicMethod,
    );
    // A buffer is its pointer and its length: two C arguments.
    accepts(&member(
        "\"initWithBytes:length:encoding:\" init_with_bytes :: ref [ubyte] -> i64u -> Maybe<Self>",
    ));
    rejects(
        &member("class \"stringWithUTF8String:\" with_utf8 :: ref utf8string -> Self"),
        DiagnosticKind::ObjcSelectorArity,
    );
    // A struct by value is one argument.
    accepts(
        "@c_layout\nrecord P { x: f64, y: f64 }\nextern class A : ObjC.Object {\n    \"moveTo:\" move_to :: P -> unit\n}\n0\n",
    );
}

#[test]
fn the_families_decide_who_owns_the_result() {
    // The expectations are clang's (ticket 現状 6): the family word, after leading underscores,
    // followed by no lowercase letter; only for an object result; `init` only on an instance.
    let source = "\
@trusted
extern class A : ObjC.Object {
    class alloc :: unit -> Self
    class \"new2Thing\" new2_thing :: unit -> A
    \"copy_x\" copy_x :: unit -> A
    \"_copyBar\" copy_bar :: unit -> A
    \"mutableCopyZ\" mutable_copy_z :: unit -> A
    \"initWithFoo:\" init_with_foo :: i64 -> Maybe<Self>
    copyfoo :: unit -> A
    newer :: unit -> A
    initialize :: unit -> A
    allocated :: unit -> A
    \"newCount\" new_count :: unit -> i64
    class \"initShared\" init_shared :: unit -> A
    @returns_retained
    borrowed :: unit -> A
    @returns_not_retained
    \"newBorrowed\" new_borrowed :: unit -> A
}
export def f :: i64 = \\() ->
    let a = A.alloc ()
    let _b = A.new2_thing ()
    let _c = A.copy_x (ref a)
    let _d = A.copy_bar (ref a)
    let _e = A.mutable_copy_z (ref a)
    let _g = A.copyfoo (ref a)
    let _h = A.newer (ref a)
    let _i = A.initialize (ref a)
    let _j = A.allocated (ref a)
    let _k = A.init_shared ()
    let _l = A.borrowed (ref a)
    let _m = A.new_borrowed (ref a)
    let count = A.new_count (ref a)
    match A.init_with_foo a 1 with
    | Maybe.Some _ -> count
    | Maybe.None -> 0
";
    let module = accepts(source);
    let retained = |member: &str| {
        module
            .functions
            .iter()
            .find_map(|function| match &function.body.kind {
                TypedExprKind::HostCall(import, _) => import
                    .objc
                    .as_ref()
                    .filter(|send| send.display == format!("A.{member}"))
                    .map(|send| send.retained),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no member {member}"))
    };
    for member in [
        "alloc",
        "new2_thing",
        "copy_x",
        "copy_bar",
        "mutable_copy_z",
        "init_with_foo",
        "borrowed",
    ] {
        assert!(retained(member), "{member} returns a +1 reference");
    }
    for member in [
        "copyfoo",
        "newer",
        "initialize",
        "allocated",
        "init_shared",
        "new_borrowed",
    ] {
        assert!(!retained(member), "{member} returns a +0 reference");
    }
    // `init` alone takes its receiver; a class method named `init…` and the others borrow.
    assert_eq!(
        member_type(&module, "A", "init_with_foo"),
        "Main.A -> i64 -> Maybe<Main.A>"
    );
    assert_eq!(member_type(&module, "A", "init_shared"), "unit -> Main.A");
    assert_eq!(member_type(&module, "A", "copy_x"), "ref Main.A -> Main.A");
    assert_eq!(member_type(&module, "A", "new_count"), "ref Main.A -> i64");

    // Only the +0 results carry the bundle; the +1 results and the scalar carry none.
    let ir = apple_ir(source);
    let calls = sends(&ir);
    assert_eq!(calls.len(), 14, "{calls:#?}");
    let bundled = calls
        .iter()
        .filter(|call| call.contains("clang.arc.attachedcall"))
        .count();
    assert_eq!(bundled, 6, "{calls:#?}");
    assert_eq!(
        ir.lines()
            .filter(|line| *line == "declare ptr @llvm.objc.retainAutoreleasedReturnValue(ptr)")
            .count(),
        1
    );
}

#[test]
fn receivers_names_and_types() {
    // `init` consumes its receiver: a second use is a use after the move.
    rejects(
        &with_cocoa(
            "\
def f :: unit -> i64u = \\() ->
    let raw = NSString.alloc ()
    let _first = NSString.init raw
    match NSString.init raw with
    | Maybe.Some s -> NSString.length (ref s)
    | Maybe.None -> 0i64u
0
",
        ),
        DiagnosticKind::UseAfterMove,
    );
    // A base's member through the derived name types the receiver and `Self` as the derived
    // class; there is no implicit conversion to the base.
    accepts(&with_cocoa(
        "\
def f :: unit -> Maybe<NSMutableString> = \\() -> NSMutableString.init (NSMutableString.alloc ())
def g :: ref NSMutableString -> i64u = \\text -> NSMutableString.length text
def h :: ref NSMutableString -> i64u = \\text -> NSString.length (ObjC.upcast text)
0
",
    ));
    assert_eq!(
        check_err(&with_cocoa(
            "def f :: unit -> Maybe<NSObject> = \\() -> NSObject.init (NSWindow.alloc ())\n0\n"
        ))
        .code,
        "E1003"
    );
    assert_eq!(
        check_err(&with_cocoa(
            "def f :: ref NSMutableString -> i64u = \\text -> NSString.length text\n0\n"
        ))
        .code,
        "E1003"
    );
    // A member that the class and its bases do not declare is no value.
    assert_eq!(
        check_err(&with_cocoa(
            "def f :: ref NSString -> i64u = \\text -> NSString.count text\n0\n"
        ))
        .code,
        "E1002"
    );
    // The same name twice in a chain, also across a base.
    rejects(
        "extern class A : ObjC.Object {\n    length :: unit -> i64\n    \"count\" length :: unit -> i64\n}\n0\n",
        DiagnosticKind::DuplicateObjcMember,
    );
    rejects(
        "extern class A : ObjC.Object {\n    length :: unit -> i64\n}\nextern class B : A {\n    length :: unit -> i64\n}\n0\n",
        DiagnosticKind::DuplicateObjcMember,
    );
    rejects(
        "extern class A : ObjC.Object\nextern category E : A {\n    length :: unit -> i64\n    \"count\" length :: unit -> i64\n}\n0\n",
        DiagnosticKind::DuplicateObjcMember,
    );
    // One selector under two names is fine.
    accepts(
        "extern class A : ObjC.Object {\n    length :: unit -> i64\n    \"length\" count :: unit -> i64\n}\n0\n",
    );
    // Only a direct call with every argument names a member.
    for value in [
        "def f :: unit -> (ref NSString -> i64u) = \\() -> NSString.length",
        "def f :: ref NSWindow -> (ref NSString -> unit) = \\window -> NSWindow.set_title window",
        "def f :: Array<NSString> -> Array<i64u> = \\texts -> Array.mapr NSString.length (ref texts)",
    ] {
        rejects(
            &with_cocoa(&format!("{value}\n0\n")),
            DiagnosticKind::ObjcMethodNotCalled,
        );
    }
    // What a message cannot carry: a record without '@c_layout' by value, a buffer or record
    // result, an owned object argument.
    let member = |text: &str| {
        format!(
            "record Plain {{ x: f64, y: f64 }}\nextern class A : ObjC.Object {{\n    {text}\n}}\n0\n"
        )
    };
    for text in [
        "\"moveTo:\" move_to :: Plain -> unit",
        "origin :: unit -> Plain",
        "bytes :: unit -> [ubyte]",
        "name :: unit -> string",
        "\"take:\" take :: Vec<i64> -> unit",
        "\"wide:\" wide :: i128 -> unit",
    ] {
        rejects(&member(text), DiagnosticKind::ObjcMemberType);
    }
    rejects(
        &member("\"adopt:\" adopt :: A -> unit"),
        DiagnosticKind::ObjcOwnedObjectArgument,
    );
    rejects(
        &member("\"swap:\" swap :: ref mut A -> unit"),
        DiagnosticKind::ObjcObjectPosition,
    );
}

#[test]
fn names_a_member_through_its_module() {
    let bindings = "\
@trusted
extern class NSObject : ObjC.Object {
    class alloc :: unit -> Self
    init :: unit -> Maybe<Self>
    hash :: unit -> i64u
}

@trusted
extern category Extras : NSObject {
    description_length :: unit -> i64u
}

private extern class Hidden : NSObject
";
    let main = "\
def f :: unit -> i64u = \\() ->
    match Cocoa.NSObject.init (Cocoa.NSObject.alloc ()) with
    | Maybe.Some object -> Cocoa.NSObject.hash (ref object) + Cocoa.Extras.description_length (ref object)
    | Maybe.None -> 0i64u
0
";
    analyze_modules(&[("Main.tz", main), ("Cocoa.tz", bindings)])
        .unwrap_or_else(|e| panic!("{}: {}", e.code, e.message));
    // A private class stays in its module.
    let hidden = "def f :: unit -> i64 = \\() ->\n    let _h = Cocoa.Hidden.alloc ()\n    1\n0\n";
    assert!(analyze_modules(&[("Main.tz", hidden), ("Cocoa.tz", bindings)]).is_err());
}

#[test]
fn lowers_with_exact_c_types() {
    let ir = apple_ir(&with_cocoa(
        "\
export def f :: i64 = \\() ->
    match NSWindow.init (NSWindow.alloc ()) with
    | Maybe.None -> 0
    | Maybe.Some window ->
        let title: NSString = ObjC.literal \"Hello\"
        NSWindow.set_title (ref window) (ref title)
        let visible = NSWindow.is_visible (ref window)
        let scale = NSWindow.backing_scale_factor (ref window)
        let alpha = NSWindow.alpha_value (ref window)
        let mut flag = false
        NSWindow.get_flag (ref window) (ref mut flag)
        let taken = NSWindow.take (ref window) 1i8 2i16 3i8u 4i16u true 1.5f32
        let narrow = NSWindow.narrow (ref window) 1i8
        let wide = NSWindow.ushorter (ref window) 2i16u
        let shown = NSWindow.title (ref window)
        let same = NSString.is_equal_to_string (ref shown) (ref title)
        let responds = NSWindow.responds (ref window) (ObjC.sel \"close\")
        let _class = NSWindow.class_object ()
        if visible && flag && same && responds then taken + (narrow as i64) + (wide as i64) + (scale as i64) + ((alpha as f64) as i64) else 0
",
    ));
    let calls = sends(&ir);
    // clang's call types for the same messages (ticket 現状 5), in the order of evaluation.
    let expected = [
        // [NSWindow alloc]: +1, no bundle. The receiver is the class.
        "call ptr @objc_msgSend(ptr %, ptr %)",
        // [x init]: consumes the receiver, +1.
        "call ptr @objc_msgSend(ptr %, ptr %)",
        "call void @objc_msgSend(ptr %, ptr %, ptr %)",
        "call zeroext i1 @objc_msgSend(ptr %, ptr %)",
        "call double @objc_msgSend(ptr %, ptr %)",
        "call float @objc_msgSend(ptr %, ptr %)",
        // BOOL *: a pointer to one byte.
        "call void @objc_msgSend(ptr %, ptr %, ptr %)",
        "call i64 @objc_msgSend(ptr %, ptr %, i8 signext 1, i16 signext 2, i8 zeroext 3, i16 zeroext 4, i1 zeroext 1, float 0x3FF8000000000000)",
        "call signext i8 @objc_msgSend(ptr %, ptr %, i8 signext 1)",
        "call zeroext i16 @objc_msgSend(ptr %, ptr %, i16 zeroext 2)",
        // [w title]: +0, retained through the bundle.
        "call ptr @objc_msgSend(ptr %, ptr %) [ \"clang.arc.attachedcall\"(ptr @llvm.objc.retainAutoreleasedReturnValue) ]",
        "call zeroext i1 @objc_msgSend(ptr %, ptr %, ptr %)",
        "call zeroext i1 @objc_msgSend(ptr %, ptr %, ptr %)",
        "call ptr @objc_msgSend(ptr %, ptr %)",
    ];
    assert_eq!(calls, expected, "{ir}");
    // `ref mut bool` is a one-byte temporary, written back as nonzero.
    assert!(ir.contains("= alloca i8, align 1"), "{ir}");
    assert!(ir.contains("icmp ne i8 %"), "{ir}");
    // The receiver of a class method is its classref; a selector is read with `!invariant.load`.
    assert!(
        ir.contains("load ptr, ptr @tz.objc.classref.NSWindow, align 8"),
        "{ir}"
    );
    let selectors = ir
        .lines()
        .filter(|line| line.contains("load ptr, ptr @tz.objc.selref."))
        .count();
    assert!(selectors >= 12, "{ir}");
    assert!(
        ir.lines()
            .filter(|line| line.contains("load ptr, ptr @tz.objc.selref."))
            .all(|line| line.trim_end().ends_with("!invariant.load !{}")),
        "{ir}"
    );
    // The selref is `externally_initialized`, so the load is not folded to the method name.
    assert!(
        ir.contains("= internal externally_initialized global ptr @tz.objc.methname."),
        "{ir}"
    );
    // One variadic declaration; no receiver is released after `init`.
    assert_eq!(
        ir.lines()
            .filter(|line| line.starts_with("declare ") && line.contains("@objc_msgSend("))
            .collect::<Vec<_>>(),
        vec!["declare ptr @objc_msgSend(ptr, ptr, ...)"],
        "{ir}"
    );
    assert!(llvm::duplicate_declarations(&ir).is_empty());
}

#[test]
fn passes_and_returns_structs_by_value() {
    // AAPCS64 as clang lowers it for arm64 macOS: an HFA is an array of its floats as an
    // argument and a struct of them as a result; 16 bytes of integers are `[2 x i64]`; a larger
    // record is a pointer to a copy as an argument and `sret` as a result.
    let ir = apple_ir(&with_cocoa(
        "\
export def f :: f64 = \\() ->
    match NSWindow.init (NSWindow.alloc ()) with
    | Maybe.None -> 0.0
    | Maybe.Some window ->
        let frame = NSWindow.frame (ref window)
        let range = NSWindow.selected_range (ref window)
        let transform = NSWindow.transform_struct (ref window)
        NSWindow.set_frame_origin (ref window) frame.origin
        NSWindow.set_transform_struct (ref window) transform
        let moved = Geometry.convert_point_to_screen (ref window) frame.origin
        let number = Geometry.number_at_point moved 0
        frame.size.width + (range.length as f64) + transform.tx + (number as f64)
",
    ));
    let calls = sends(&ir);
    let expected = [
        "call ptr @objc_msgSend(ptr %, ptr %)",
        "call ptr @objc_msgSend(ptr %, ptr %)",
        "call { double, double, double, double } @objc_msgSend(ptr %, ptr %)",
        "call [2 x i64] @objc_msgSend(ptr %, ptr %)",
        "call void @objc_msgSend(ptr sret([48 x i8]) %, ptr %, ptr %)",
        "call void @objc_msgSend(ptr %, ptr %, [2 x double] %)",
        "call void @objc_msgSend(ptr %, ptr %, ptr %)",
        "call { double, double } @objc_msgSend(ptr %, ptr %, [2 x double] %)",
        "call i64 @objc_msgSend(ptr %, ptr %, [2 x double] %, i64 0)",
    ];
    assert_eq!(calls, expected, "{ir}");
    // The receiver is never nil by its type, so no memset zeroes an `sret` result before a send.
    assert!(!ir.contains("alignstack"), "{ir}");
}

#[test]
fn shares_the_declaration_of_objc_msgsend_with_the_program() {
    // The program's own untyped `objc_msgSend` (the W14 rule) keeps its declaration; the typed
    // sends call the same symbol with their own types.
    let ir = apple_ir(&with_cocoa(
        "\
@trusted
extern \"objc_msgSend\" def send0 :: i64 -> i64 -> i64

export def f :: i64u = \\() ->
    match NSString.init (NSString.alloc ()) with
    | Maybe.Some text -> NSString.length (ref text) + ((send0 0 0) as i64u)
    | Maybe.None -> 0i64u
",
    ));
    assert_eq!(
        ir.lines()
            .filter(|line| line.starts_with("declare ") && line.contains("@objc_msgSend("))
            .collect::<Vec<_>>(),
        vec!["declare i64 @objc_msgSend(i64, i64)"],
        "{ir}"
    );
    assert!(ir.contains("call i64 @objc_msgSend(ptr %"), "{ir}");
    assert!(llvm::duplicate_declarations(&ir).is_empty());
}

#[test]
fn constant_strings_have_clangs_layout() {
    let ir = apple_ir(&with_cocoa(
        "\
export def f :: i64u = \\() ->
    let hello: NSString = ObjC.literal \"Hello\"
    let again: NSObject = ObjC.literal \"Hello\"
    let empty: ObjC.Object = ObjC.literal \"\"
    let wide: NSString = ObjC.literal \"日本\"
    let accent: NSString = ObjC.literal \"café\"
    let nul: NSString = ObjC.literal \"a\\0b\"
    let _unused = (again, empty)
    NSString.length (ref hello) + NSString.length (ref wide) + NSString.length (ref accent) + NSString.length (ref nul)
",
    ));
    assert!(
        ir.contains("@__CFConstantStringClassReference = external global [0 x i32]\n"),
        "{ir}"
    );
    // ASCII without a NUL: bytes, flags 1992, the length in bytes. The same text is one object.
    assert!(
        ir.contains(
            "@tz.objc.cfstring.data.0 = private unnamed_addr constant [6 x i8] c\"\\48\\65\\6C\\6C\\6F\\00\", section \"__TEXT,__cstring,cstring_literals\", align 1\n"
        ),
        "{ir}"
    );
    assert!(
        ir.contains(
            "@tz.objc.cfstring.0 = private global { ptr, i32, ptr, i64 } { ptr @__CFConstantStringClassReference, i32 1992, ptr @tz.objc.cfstring.data.0, i64 5 }, section \"__DATA,__cfstring\", align 8\n"
        ),
        "{ir}"
    );
    // The empty string.
    assert!(
        ir.contains("@tz.objc.cfstring.data.1 = private unnamed_addr constant [1 x i8] c\"\\00\""),
        "{ir}"
    );
    assert!(
        ir.contains("i32 1992, ptr @tz.objc.cfstring.data.1, i64 0 }"),
        "{ir}"
    );
    // Anything else is UTF-16 with a final zero unit: flags 2000, the length in units.
    assert!(
        ir.contains(
            "@tz.objc.cfstring.data.2 = private unnamed_addr constant [3 x i16] [i16 26085, i16 26412, i16 0], section \"__TEXT,__ustring\", align 2\n"
        ),
        "{ir}"
    );
    assert!(
        ir.contains("i32 2000, ptr @tz.objc.cfstring.data.2, i64 2 }"),
        "{ir}"
    );
    assert!(
        ir.contains("[5 x i16] [i16 99, i16 97, i16 102, i16 233, i16 0]"),
        "{ir}"
    );
    assert!(
        ir.contains("i32 2000, ptr @tz.objc.cfstring.data.3, i64 4 }"),
        "{ir}"
    );
    // An ASCII text with a NUL inside is UTF-16 too.
    assert!(
        ir.contains("[4 x i16] [i16 97, i16 0, i16 98, i16 0]"),
        "{ir}"
    );
    assert!(
        ir.contains("i32 2000, ptr @tz.objc.cfstring.data.4, i64 3 }"),
        "{ir}"
    );
    assert_eq!(ir.matches("@tz.objc.cfstring.data.").count() / 2, 5, "{ir}");

    // The expected type is NSString, one of its bases, or ObjC.Object.
    rejects(
        &with_cocoa("def f :: unit -> NSMutableString = \\() -> ObjC.literal \"x\"\n0\n"),
        DiagnosticKind::ObjcLiteralType,
    );
    rejects(
        &with_cocoa("def f :: unit -> NSWindow = \\() -> ObjC.literal \"x\"\n0\n"),
        DiagnosticKind::ObjcLiteralType,
    );
    rejects(
        &with_cocoa("def f :: unit -> i64 = \\() -> ObjC.literal \"x\"\n0\n"),
        DiagnosticKind::ObjcLiteralType,
    );
    rejects(
        &with_cocoa(
            "def f :: unit -> i64 = \\() ->\n    let _text = ObjC.literal \"x\"\n    1\n0\n",
        ),
        DiagnosticKind::AmbiguousPolymorphicUse,
    );
    // The argument is a literal without interpolation; a constant is no literal.
    rejects(
        &with_cocoa("def f :: i64 -> NSString = \\n -> ObjC.literal $\"n{n}\"\n0\n"),
        DiagnosticKind::ObjcLiteralArgument,
    );
    rejects(
        &with_cocoa("def f :: string -> NSString = \\text -> ObjC.literal text\n0\n"),
        DiagnosticKind::ObjcLiteralArgument,
    );
    rejects(
        &with_cocoa(
            "const NAME: string = \"x\"\ndef f :: unit -> NSString = \\() -> ObjC.literal NAME\n0\n",
        ),
        DiagnosticKind::ObjcLiteralArgument,
    );
    rejects(
        &with_cocoa("def f :: unit -> (string -> NSString) = \\() -> ObjC.literal\n0\n"),
        DiagnosticKind::ObjcBuiltinNotCalled,
    );
}

#[test]
fn only_an_apple_target_sends_messages() {
    let reached = with_cocoa(
        "export def f :: i64 = \\() ->\n    let _class = NSObject.class_object ()\n    1\n",
    );
    let literal = with_cocoa(
        "@trusted\nextern \"use\" def use_object :: ref NSString -> i64\nexport def f :: i64 = \\() ->\n    let text: NSString = ObjC.literal \"x\"\n    use_object (ref text)\n",
    );
    for source in [&reached, &literal] {
        let module = accepts(source);
        let wasm = llvm::emit_target(&module, llvm::Entry::Library, true).unwrap_err();
        assert_eq!(wasm.kind, Some(DiagnosticKind::AbiNativeOnly.id()));
        for triple in [Triple::X86_64LinuxGnu, Triple::Aarch64WindowsMsvc] {
            let _guard = PlatformScope::enter(Platform::new(Some(triple)));
            let other = llvm::emit(&module, llvm::Entry::Library).unwrap_err();
            assert_eq!(other.kind, Some(DiagnosticKind::AbiAppleOnly.id()));
        }
    }
    // Declarations with members that nothing sends change no output on any target.
    let declared = "\
@trusted
extern class NSObject : ObjC.Object {
    class alloc :: unit -> Self
    init :: unit -> Maybe<Self>
    hash :: unit -> i64u
}

@trusted
extern category Extras : NSObject {
    \"isProxy\" is_proxy :: unit -> bool
}

export def f :: i64 -> i64 = \\x -> x + 1
";
    let plain = "export def f :: i64 -> i64 = \\x -> x + 1\n";
    let (module, bare) = (accepts(declared), accepts(plain));
    assert_eq!(
        llvm::emit_target(&module, llvm::Entry::Library, true).unwrap(),
        llvm::emit_target(&bare, llvm::Entry::Library, true).unwrap()
    );
    for triple in [Triple::X86_64LinuxGnu, Triple::Aarch64AppleDarwin] {
        let _guard = PlatformScope::enter(Platform::new(Some(triple)));
        assert_eq!(
            llvm::emit(&module, llvm::Entry::Library).unwrap(),
            llvm::emit(&bare, llvm::Entry::Library).unwrap()
        );
    }
}

#[test]
fn a_spawning_program_wraps_its_sends() {
    let body = "\
def work :: unit -> i64u = \\() ->
    match NSString.init (NSString.alloc ()) with
    | Maybe.Some text -> NSString.length (ref text)
    | Maybe.None -> 0i64u
";
    let spawning = with_cocoa(&format!(
        "{body}\ndef main :: unit -> i32 = \\() ->\n    let _n = work ()\n    tzroutine {{\n        ()\n    }}\n    0\n"
    ));
    let plain = with_cocoa(&format!(
        "{body}\ndef main :: unit -> i32 = \\() ->\n    let _n = work ()\n    0\n"
    ));
    let _guard = PlatformScope::enter(Platform::new(Some(Triple::Aarch64AppleDarwin)));
    let wrapped = llvm::emit(&accepts(&spawning), llvm::Entry::Console).unwrap();
    let lines: Vec<&str> = wrapped.lines().collect();
    let mut seen = 0;
    for (index, line) in lines.iter().enumerate() {
        if line.contains("@objc_msgSend(") && !line.starts_with("declare") {
            assert!(
                lines[index - 1].contains("@tsuzuri_tz_blocking_enter"),
                "{}\n{line}",
                lines[index - 1]
            );
            assert!(
                lines[index + 1].contains("@tsuzuri_tz_blocking_exit"),
                "{line}\n{}",
                lines[index + 1]
            );
            seen += 1;
        }
    }
    assert_eq!(seen, 3, "{wrapped}");
    // A program that starts no tzroutine wraps nothing.
    let bare = llvm::emit(&accepts(&plain), llvm::Entry::Console).unwrap();
    assert!(!bare.contains("@tsuzuri_tz_blocking_enter"), "{bare}");
    assert_eq!(sends(&bare).len(), 3);
}

#[test]
fn the_audit_counts_sends_on_their_class_and_category() {
    let untrusted = "\
extern class NSObject : ObjC.Object {
    class alloc :: unit -> Self
    init :: unit -> Maybe<Self>
    hash :: unit -> i64u
}

@trusted
extern category Extras : NSObject {
    \"isProxy\" is_proxy :: unit -> bool
}

extern category Unused : NSObject {
    \"zone\" zone :: unit -> i64
}

def main :: unit -> i32 = \\() ->
    match NSObject.init (NSObject.alloc ()) with
    | Maybe.Some object ->
        let _hash = NSObject.hash (ref object)
        if Extras.is_proxy (ref object) then 1 else 0
    | Maybe.None -> 2
";
    let root = temp_project("audit", &[("Main.tz", untrusted)]);
    let output = Command::new(bin())
        .arg("audit")
        .arg(&root)
        .arg("--json")
        .env("TSUZURI_CACHE_DIR", "")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    // One row for the class with its three sends, one for each category; no row for a member.
    let row = |name: &str| {
        text.lines()
            .find(|line| {
                line.contains("\"type\":\"extern\"")
                    && line.contains(&format!("\"name\":\"{name}\""))
            })
            .unwrap_or_else(|| panic!("no row for {name}:\n{text}"))
            .to_owned()
    };
    let class = row("NSObject");
    assert!(
        class.contains("\"symbol\":\"OBJC_CLASS_$_NSObject\""),
        "{class}"
    );
    assert!(class.contains("\"used\":3"), "{class}");
    assert!(class.contains("\"trusted\":false"), "{class}");
    let extras = row("Extras");
    assert!(
        extras.contains("\"symbol\":\"OBJC_CLASS_$_NSObject\""),
        "{extras}"
    );
    assert!(extras.contains("\"used\":1"), "{extras}");
    assert!(extras.contains("\"trusted\":true"), "{extras}");
    assert!(row("Unused").contains("\"used\":0"), "{text}");
    assert!(!text.contains("$objc"), "{text}");
    assert!(text.contains("\"capability\":\"ffi\""), "{text}");

    // The class that is used without '@trusted' gets the warning; the unused category does not.
    let checked = Command::new(bin())
        .arg("check")
        .arg(&root)
        .args(["--warn", "untrusted-extern", "--json"])
        .env("TSUZURI_CACHE_DIR", "")
        .output()
        .unwrap();
    let warnings = String::from_utf8_lossy(&checked.stdout).into_owned()
        + &String::from_utf8_lossy(&checked.stderr);
    assert!(warnings.contains("W1010"), "{warnings}");
    assert!(
        warnings.contains("extern class 'NSObject' is used but not marked '@trusted'"),
        "{warnings}"
    );
    assert!(!warnings.contains("'Unused'"), "{warnings}");
    assert!(!warnings.contains("'Extras'"), "{warnings}");
    fs::remove_dir_all(root).ok();
}
