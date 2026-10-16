use std::collections::BTreeSet;
use tsuzuri::{analyze, llvm, trap::TrapSource};

const SOURCE: &str = "\
def used :: i64 -> i64 = \\x -> x + 1

def unused :: i64 -> i64 = \\x -> x * 3

export def exported :: i64 -> i64 = \\x -> x - 1

used 41
";

fn options(entry: llvm::Entry) -> llvm::EmitOptions {
    llvm::EmitOptions {
        entry,
        wasm: false,
        wasm_simd128: false,
        debug_output: false,
        runtime_objects: false,
        allocator: llvm::Allocator::System,
        f16: llvm::F16Path::Soft,
    }
}

fn sources(text: &str) -> Vec<TrapSource<'_>> {
    std::iter::once(TrapSource {
        path: "Main.tz",
        text,
    })
    .chain(
        tsuzuri::stdlib::SOURCES
            .iter()
            .map(|(path, text)| TrapSource { path, text }),
    )
    .collect()
}

/// The names that `ir` defines, such as `@tz.fn.Main.used`.
fn defined(ir: &str) -> BTreeSet<String> {
    ir.lines()
        .filter(|line| line.starts_with("define "))
        .filter_map(|line| {
            let start = line.find('@')? + 1;
            let end = start + line[start..].find('(')?;
            Some(line[start..end].to_owned())
        })
        .collect()
}

/// The lines of `ir` that define a named type, such as `%tz.string = type { ptr, i64 }`.
fn type_definitions(ir: &str) -> BTreeSet<String> {
    ir.lines()
        .filter(|line| line.starts_with('%') && line.contains(" = type "))
        .map(str::to_owned)
        .collect()
}

/// The record, union and node types that `ir` names without defining them, which Clang rejects
/// (`load operand must be a pointer to a first class type`).
fn undefined_types(ir: &str) -> BTreeSet<String> {
    let defined: BTreeSet<String> = type_definitions(ir)
        .iter()
        .filter_map(|line| line.split_once(" = type "))
        .map(|(name, _)| name.to_owned())
        .collect();
    let mut used = BTreeSet::new();
    let mut at = 0;
    while let Some(found) = ir[at..].find('%') {
        let start = at + found;
        let rest = &ir[start + 1..];
        let length = if let Some(quoted) = rest.strip_prefix('"') {
            quoted.find('"').map(|end| end + 2)
        } else {
            Some(
                rest.find(|c: char| !(c.is_ascii_alphanumeric() || "._$-".contains(c)))
                    .unwrap_or(rest.len()),
            )
        };
        let name = &ir[start..start + 1 + length.unwrap_or(0)];
        if name.starts_with("%\"tz.") || name.starts_with("%tz.record.") {
            used.insert(name.to_owned());
        }
        at = start + 1;
    }
    used.difference(&defined).cloned().collect()
}

fn executable(text: &str, trap_info: bool) -> llvm::EmitOutput {
    let module = analyze(text).unwrap();
    llvm::emit_native_build(
        &module,
        options(llvm::Entry::Console),
        &sources(text),
        None,
        trap_info,
    )
    .unwrap()
}

fn unpruned(text: &str, trap_info: bool) -> llvm::EmitOutput {
    let module = analyze(text).unwrap();
    llvm::emit_native_build_for(
        &module,
        options(llvm::Entry::Console),
        &sources(text),
        None,
        trap_info,
        0,
    )
    .unwrap()
}

#[test]
fn native_executables_emit_only_entry_and_export_reachable_functions() {
    let ir = executable(SOURCE, false).ir;
    let names = defined(&ir);
    for name in [
        "tz.fn.Main.used",
        "tz.fn.Main.exported",
        "tz_exported",
        "main",
    ] {
        assert!(names.contains(name), "{name} is defined: {names:?}");
    }
    assert!(!ir.contains("@tz.fn.Main.unused"), "{names:?}");
    assert!(!ir.contains("@tz.apply.Main.unused"));
    // The same input gives the same text.
    assert_eq!(ir, executable(SOURCE, false).ir);
}

#[test]
fn library_and_plain_emission_keep_public_functions() {
    let module = analyze(SOURCE).unwrap();
    let kept = |ir: &str| defined(ir).contains("tz.fn.Main.unused");
    for entry in [llvm::Entry::Library, llvm::Entry::Console] {
        let build =
            llvm::emit_native_build_for(&module, options(entry), &sources(SOURCE), None, false, 0)
                .unwrap();
        assert!(
            kept(&build.ir),
            "emit_native_build_for keeps every public function"
        );
        assert!(kept(&llvm::emit(&module, entry).unwrap()));
        assert!(kept(
            &llvm::emit_with_options(&module, options(entry)).unwrap()
        ));
    }
    let library = llvm::emit_native_build(
        &module,
        options(llvm::Entry::Library),
        &sources(SOURCE),
        None,
        false,
    )
    .unwrap();
    assert!(kept(&library.ir), "a library is not an executable");
}

#[test]
fn pruning_only_removes_unreachable_definitions() {
    let pruned = defined(&executable(SOURCE, false).ir);
    let full = defined(&unpruned(SOURCE, false).ir);
    let removed: Vec<_> = full.difference(&pruned).cloned().collect();
    // The function and the closure body of its lambda.
    assert_eq!(removed, ["tz.apply.Main.unused.0", "tz.fn.Main.unused"]);
    assert!(pruned.is_subset(&full));
}

#[test]
fn a_program_that_uses_every_public_function_is_unchanged() {
    let source = "def double :: i64 -> i64 = \\x -> x + x\n\nexport def twice :: i64 -> i64 = \\x -> double x\n\ndouble 21\n";
    assert_eq!(executable(source, false).ir, unpruned(source, false).ir);
    assert_eq!(executable(source, true).ir, unpruned(source, true).ir);
}

#[test]
fn callees_of_the_entry_and_of_exports_survive_through_closures_and_helpers() {
    let source = "\
def inner :: i64 -> i64 = \\x -> x + 7

def outer :: i64 -> i64 = \\x -> inner x

export def exported :: i64 -> i64 = \\x -> outer x

def via_closure :: i64 -> i64 = \\x -> (\\y -> helper y) x

def helper :: i64 -> i64 = \\x -> x * 2

def orphan :: i64 -> i64 = \\x -> x + 100

via_closure 1
";
    let pruned = defined(&executable(source, false).ir);
    let names: Vec<_> = pruned
        .iter()
        .filter(|name| name.starts_with("tz.fn.Main."))
        .collect();
    for needed in ["inner", "outer", "exported", "via_closure", "helper"] {
        assert!(
            names
                .iter()
                .any(|name| name.as_str() == format!("tz.fn.Main.{needed}")),
            "{needed} is reachable: {names:?}"
        );
    }
    assert!(!pruned.contains("tz.fn.Main.orphan"), "{names:?}");
}

#[test]
fn trap_sites_of_removed_functions_are_gone_and_the_rest_are_kept() {
    let source = "\
def divide :: i64 -> i64 -> i64 = \\a b -> a / b

def never_called :: i64 -> i64 -> i64 = \\a b -> a / b

export def exported :: i64 -> i64 -> i64 = \\a b -> divide a b

divide 84 2
";
    let pruned = executable(source, true);
    let full = unpruned(source, true);
    assert!(!pruned.trap_sites.is_empty());
    assert!(
        pruned.trap_sites.len() < full.trap_sites.len(),
        "{} sites of {}",
        pruned.trap_sites.len(),
        full.trap_sites.len()
    );
    assert!(!pruned.ir.contains("@tz.fn.Main.never_called"));
    assert!(full.ir.contains("@tz.fn.Main.never_called"));
}

#[test]
fn unused_public_functions_are_still_checked() {
    let source = SOURCE.replace("x * 3", "true");
    let error = analyze(&source).unwrap_err();
    assert_eq!(error.code, "E1003", "{error:?}");
    assert!(
        error.message.contains("expected i64, found bool"),
        "{error:?}"
    );
    let start = source.find("true").unwrap();
    assert!(
        error.span.start <= start && start < error.span.end,
        "{:?} should cover offset {start}",
        error.span
    );
}

#[test]
fn user_drop_glue_stays_a_root_of_every_executable() {
    let drop = |names: &BTreeSet<String>| {
        names
            .iter()
            .filter(|name| name.starts_with("tz.fn.Main.$instance.") && name.ends_with(".drop"))
            .count()
    };
    let reached = "\
record Resource { id: i64 }

instance Drop<Resource> {
    fn drop _value = ()
}

def use_resource :: i64 -> i64 = \\x ->
    let resource = Resource { id: x }
    resource.id + 1

use_resource 41
";
    assert_eq!(drop(&defined(&executable(reached, false).ir)), 1);
    assert_eq!(executable(reached, false).ir, unpruned(reached, false).ir);
    // Drop glue calls the user drop without a reference in any body, so it is never pruned.
    let unreached = reached.replace("use_resource 41", "0");
    assert_eq!(drop(&defined(&executable(&unreached, false).ir)), 1);
}

/// A public function that nothing reaches names the element type of a channel. The drops of the
/// channel ends are still roots, and their builtins load that type.
const CHANNEL: &str = "\
def helper :: i64
fn helper =
    let pair: (Channel.Sender<Maybe<i64>> * Channel.Receiver<Maybe<i64>>) = Channel.bounded 2
    match pair with
    | (sender, receiver) ->
        let _sent = Channel.send (ref sender) (Maybe.Some 41i64)
        match Channel.recv (ref receiver) with
        | Maybe.Some (Maybe.Some v) -> v + 1
        | _ -> 0

\"ok\"
";

/// The same with a user generic record as the element.
const CASED: &str = "\
record Cased<'a> { value: 'a }

def helper :: i64
fn helper =
    let pair: (Channel.Sender<Cased<i64>> * Channel.Receiver<Cased<i64>>) = Channel.bounded 2
    match pair with
    | (sender, receiver) ->
        let _sent = Channel.send (ref sender) (Cased { value: 41i64 })
        match Channel.recv (ref receiver) with
        | Maybe.Some held -> held.value + 1
        | Maybe.None -> 0

\"ok\"
";

#[test]
fn the_types_that_a_rooted_drop_loads_stay_defined_when_only_a_removed_function_names_them() {
    for (name, source) in [("std element", CHANNEL), ("user generic element", CASED)] {
        for trap_info in [false, true] {
            let pruned = executable(source, trap_info).ir;
            assert!(
                !defined(&pruned).contains("tz.fn.Main.helper"),
                "{name}: the unused function is pruned"
            );
            assert_eq!(
                undefined_types(&pruned),
                BTreeSet::new(),
                "{name} (trap info {trap_info}): every named type has a definition"
            );
            assert_eq!(
                undefined_types(&unpruned(source, trap_info).ir),
                BTreeSet::new(),
                "{name}: the check accepts the unpruned program"
            );
        }
    }
}

#[test]
fn pruning_removes_no_type_definition() {
    // A definition costs a line, and a drop or a builtin can load a type that no emitted body names,
    // so the executable defines every type that the unpruned program defines.
    let pair = "\
record Pair<'a, 'b> { left: 'a, right: 'b }

union Shape =
    | Dot of i64
    | Span of Pair<i64, string>

def only_here :: i64 -> Pair<i64, string> = \\x -> Pair { left: x, right: \"x\" }

def shapes :: i64 -> Maybe<Shape> = \\x -> Maybe.Some (Shape.Span (only_here x))

\"ok\"
";
    for source in [SOURCE, CHANNEL, CASED, pair] {
        for trap_info in [false, true] {
            let pruned = executable(source, trap_info).ir;
            let full = unpruned(source, trap_info).ir;
            assert_eq!(
                type_definitions(&pruned),
                type_definitions(&full),
                "{source}"
            );
            assert!(defined(&pruned).is_subset(&defined(&full)));
        }
    }
}

/// The functions of `ir` that use `%tz.context`, the trap-site parameter of the functions that the instrumentation
/// manages, without taking it: Clang rejects them (`use of undefined value '%tz.context'`).
fn context_without_parameter(ir: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut open: Option<(String, bool)> = None;
    for raw in ir.lines() {
        // A runtime part can follow the closing brace of the function before it on the same line.
        let line = match raw.strip_prefix('}') {
            Some(rest) if !rest.is_empty() => {
                open = None;
                rest
            }
            _ => raw,
        };
        if line.starts_with("define ") {
            let start = line.find('@').unwrap() + 1;
            let name = line[start..start + line[start..].find('(').unwrap()].to_owned();
            open = Some((name, line.contains("%tz.context")));
        } else if line == "}" {
            open = None;
        } else if let Some((name, false)) = &open
            && line.contains("%tz.context")
        {
            found.insert(name.clone());
        }
    }
    found
}

/// A recursive union and a displayed list make the runtime parts `recursive.ll` and `display.ll`, and the emitter
/// appends a part right behind the previous one. `recursive.ll` ends without a line break, so `display.ll` starts in
/// the line of its last brace (`}define ...`) unless something else is appended between them.
const TREE: &str = "\
union Tree =
    | Leaf
    | Node of Tree * i64 * Tree

def rec size :: Tree -> i64 = \\tree ->
    match tree with
    | Tree.Leaf -> 0
    | Tree.Node (left, _, right) -> size left + 1 + size right

let values = [1, 2, 3]
$\"{values} {size (Tree.Node (Tree.Leaf, 1, Tree.Leaf))}\"
";

/// The same program with an unused public function that locks a `Mutex`, whose runtime part (`sync.ll`) stands between the two
/// runtime parts until the function is pruned.
const TREE_WITH_LOCK: &str = "\
union Tree =
    | Leaf
    | Node of Tree * i64 * Tree

def rec size :: Tree -> i64 = \\tree ->
    match tree with
    | Tree.Leaf -> 0
    | Tree.Node (left, _, right) -> size left + 1 + size right

def bump :: i64 -> i64 = \\n ->
    let total = Mutex.create 0i64
    let _seen = Task.scope (ref total) 4 (shared -> index -> Mutex.with_lock shared (value -> {
        deref value = deref value + index + n;
        index
    }))
    Mutex.into_inner total

let values = [1, 2, 3]
$\"{values} {size (Tree.Node (Tree.Leaf, 1, Tree.Leaf))}\"
";

#[test]
fn the_trap_instrumentation_reads_runtime_parts_that_share_a_line() {
    // `display.ll` follows `recursive.ll` in the line of its last brace, which the baseline compiler already
    // instrumented wrongly: the function took no `%tz.context` and its body used it.
    for ir in [unpruned(TREE, true).ir, executable(TREE, true).ir] {
        assert_eq!(context_without_parameter(&ir), BTreeSet::new());
    }
}

#[test]
fn an_unused_function_does_not_decide_whether_a_trap_info_build_still_works() {
    // The lock stands between the two runtime parts, so the unpruned program (which `tsuzuri run` built before
    // pruning) has the line break that the pruned one lost with the function.
    assert_eq!(
        context_without_parameter(&unpruned(TREE_WITH_LOCK, true).ir),
        BTreeSet::new()
    );
    let pruned = executable(TREE_WITH_LOCK, true).ir;
    assert!(!defined(&pruned).contains("tz.fn.Main.bump"));
    assert_eq!(context_without_parameter(&pruned), BTreeSet::new());
}
