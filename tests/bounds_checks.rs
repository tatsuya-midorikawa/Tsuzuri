use tsuzuri::llvm::{EmitOptions, EmitOutput, Entry, emit_with_options, emit_with_trap_info};
use tsuzuri::trap::{TrapKind, TrapSource};

const FORWARD: &str = r"export def shape :: i64 -> i64
fn shape count =
    assert (count >= 0)
    let values = new [i64](count, \i -> i * 3 + 1)
    let mut total = 0
    for i in 0 .. values.length - 1 do
        total = total + values[i]
    total
";

const BACKWARD: &str = r"export def shape :: i64 -> i64
fn shape count =
    assert (count >= 0)
    let values = new [i64](count, \i -> i * 3 + 1)
    let mut total = 0
    for i in values.length - 1 .. -1 .. 0 do
        total = total + values[i]
    total
";

const STANDARD_LENGTH: &str = r"def sum_all :: ref [i64] -> i64
fn sum_all values =
    let mut total = 0
    for i in 0 .. Array.length values - 1 do
        total = total + values[i]
    total

export def shape :: i64 -> i64
fn shape count =
    assert (count >= 0)
    let values = new [i64](count, \i -> i * 3 + 1)
    sum_all (ref values)
";

const LITERAL: &str = r"export def shape :: i64 -> i64
fn shape seed =
    assert (seed >= 0)
    let values = [10, 20, 30]
    values[2] + values[0] + seed
";

const CONSTANT_LOOP: &str = r"export def shape :: i64 -> i64
fn shape seed =
    assert (seed >= 0)
    let values = [10, 20, 30]
    let mut total = seed
    for i in 0 .. 2 do
        total = total + values[i]
    total
";

const GUARDED: &str = r"export def shape :: i64 -> i64
fn shape index =
    assert (index != 7)
    let values = [10, 20, 30]
    let guarded = if index >= 0 && index < values.length then values[index] else 0
    values[2] + guarded
";

const TAIL_GUARD: &str = r"def rec walk :: i64 -> i64 -> i64
fn rec walk i total =
    let values = [10, 20, 30]
    if i >= 0 && i < values.length then walk (i + 1) (total + values[i]) else total

export def shape :: i64 -> i64
fn shape start =
    assert (start != 7)
    walk start 0
";

const BOUND: &str = r"export def shape :: i64 -> i64
fn shape index =
    let values = [1, 2]
    values[index]
";

const REPLACED: &str = r"export def shape :: i64 -> i64
fn shape count =
    let mut values = new [i64](count, \i -> i * 3 + 1)
    let mut total = 0
    for i in 0 .. values.length - 1 do
        values = [0]
        total = total + values[i]
    total
";

const OTHER: &str = r"export def shape :: i64 -> i64
fn shape count =
    let a = new [i64](count, \i -> i * 3 + 1)
    let b = new [i64](0, \i -> i)
    let mut total = 0
    for i in 0 .. a.length - 1 do
        total = total + b[i]
    total
";

const PAST_END: &str = r"export def shape :: i64 -> i64
fn shape count =
    let values = new [i64](count, \i -> i * 3 + 1)
    let mut total = 0
    for i in 0 .. values.length do
        total = total + values[i]
    total
";

const WHILE: &str = r"export def shape :: i64 -> i64
fn shape count =
    let values = new [i64](count, \i -> i * 3 + 1)
    let mut total = 0
    let mut i = 0
    while i < values.length do
        total = total + values[i]
        i = i + 1
    total
";

const LITERAL_PAST: &str = r"export def shape :: i64 -> i64
fn shape seed =
    let values = [10, 20, 30]
    values[3] + seed
";

const PROVEN: [(&str, &str); 7] = [
    ("forward", FORWARD),
    ("backward", BACKWARD),
    ("standard length", STANDARD_LENGTH),
    ("literal", LITERAL),
    ("constant loop", CONSTANT_LOOP),
    ("guarded", GUARDED),
    ("tail guard", TAIL_GUARD),
];

const RETAINED: [(&str, &str); 6] = [
    ("bound", BOUND),
    ("replaced", REPLACED),
    ("other", OTHER),
    ("past end", PAST_END),
    ("while", WHILE),
    ("literal past", LITERAL_PAST),
];

fn emitted(source: &str, wasm: bool) -> EmitOutput {
    let module = tsuzuri::analyze(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    let sources = [TrapSource {
        path: "Main.tz",
        text: source,
    }];
    let options = EmitOptions {
        entry: Entry::Library,
        wasm,
        debug_output: false,
    };
    let first = emit_with_trap_info(&module, options, &sources).unwrap();
    let second = emit_with_trap_info(&module, options, &sources).unwrap();
    assert_eq!(first.ir, second.ir, "IR is deterministic\n{source}");
    assert_eq!(first.trap_sites, second.trap_sites);
    first
}

/// Source text of every `kind` trap site, sorted; native and WASM must agree.
fn sites(source: &str, kind: TrapKind) -> Vec<String> {
    let mut targets = [false, true].map(|wasm| {
        let mut texts: Vec<_> = emitted(source, wasm)
            .trap_sites
            .iter()
            .filter(|site| site.kind == kind)
            .map(|site| source[site.span.start..site.span.end].to_owned())
            .collect();
        texts.sort();
        texts
    });
    assert_eq!(targets[0], targets[1], "native and WASM agree\n{source}");
    std::mem::take(&mut targets[0])
}

#[test]
fn proven_indices_emit_no_bounds_guard() {
    for (name, source) in PROVEN {
        assert_eq!(
            sites(source, TrapKind::BoundsCheck),
            Vec::<String>::new(),
            "{name}"
        );
    }
}

#[test]
fn unproven_indices_keep_bounds_guards_and_sites() {
    // Site texts recorded with the compiler before F12; the function name is the call-context site.
    let expected: [&[&str]; 6] = [
        &["shape", "values[index]"],
        &["shape", "values[i]"],
        &["b[i]", "shape"],
        &["shape", "values[i]"],
        &["shape", "values[i]"],
        &["shape", "values[3]"],
    ];
    for ((name, source), expected) in RETAINED.into_iter().zip(expected) {
        assert_eq!(sites(source, TrapKind::BoundsCheck), expected, "{name}");
    }
}

#[test]
fn other_trap_kinds_are_unchanged() {
    const CREATE: &str = "new [i64](count, \\i -> i * 3 + 1)";
    // (allocation size sites, assertion sites) recorded with the compiler before F12.
    let expected: [(&[&str], &[&str]); 7] = [
        (&[CREATE, "shape"], &["assert (count >= 0)", "shape"]),
        (&[CREATE, "shape"], &["assert (count >= 0)", "shape"]),
        (&[CREATE, "shape"], &["assert (count >= 0)", "shape"]),
        (&[], &["assert (seed >= 0)", "shape"]),
        (&[], &["assert (seed >= 0)", "shape"]),
        (&[], &["assert (index != 7)", "shape"]),
        (&[], &["assert (start != 7)", "shape"]),
    ];
    for ((name, source), (allocation, assertion)) in PROVEN.into_iter().zip(expected) {
        assert_eq!(
            sites(source, TrapKind::AllocationSize),
            allocation,
            "{name}"
        );
        assert_eq!(sites(source, TrapKind::Assert), assertion, "{name}");
    }
}

#[test]
fn range_proofs_add_no_llvm_facts() {
    for (name, source) in PROVEN {
        let module = tsuzuri::analyze(source).unwrap();
        let options = EmitOptions {
            entry: Entry::Library,
            wasm: false,
            debug_output: false,
        };
        let ir = emit_with_options(&module, options).unwrap();
        for fact in ["llvm.assume", "!range", " nsw ", " nuw "] {
            assert!(!ir.contains(fact), "{name}: {fact}");
        }
        assert!(
            ir.contains("getelementptr inbounds i64"),
            "{name}: elements are still addressed"
        );
    }
}

#[test]
fn unroll_hint_is_dropped_only_where_a_guard_was_removed() {
    // A reduction loop without its bounds guard is vectorized best when LLVM is not asked to unroll it first.
    const COUNTING: &str = r"export def shape :: i64 -> i64
fn shape count =
    assert (count >= 0)
    let mut total = 0
    for i in 0 .. count - 1 do
        total = total + i * 3
    total
";
    const HINT: &str = "llvm.loop.unroll.enable";
    let ir = |source: &str| {
        let module = tsuzuri::analyze(source).unwrap();
        let options = EmitOptions {
            entry: Entry::Library,
            wasm: false,
            debug_output: false,
        };
        emit_with_options(&module, options).unwrap()
    };
    assert!(!ir(FORWARD).contains(HINT), "proven reads");
    assert!(ir(PAST_END).contains(HINT), "guard kept");
    assert!(ir(COUNTING).contains(HINT), "no array reads");
}
