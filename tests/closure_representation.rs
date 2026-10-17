//! Function values (PM02): the shape of their IR and the size model behind the layout limit.
use std::collections::BTreeSet;
use tsuzuri::{analyze, analyze_modules, llvm};

/// Four kinds of function values: a function used as a value, a capture that fits a pointer, an
/// environment on the heap, and a partial application.
const MIX: &str = "def inc :: i64 -> i64
fn inc x = x + 1

def add3 :: i64 -> i64 -> i64 -> i64
fn add3 a b c = a + b + c

export def closure_mix :: i64 -> i64
fn closure_mix seed =
    let prefix = \"abc\"
    let plain: i64 -> i64 = inc
    let immediate: i64 -> i64 = \\x -> x + seed
    let heap: i64 -> i64 = \\x -> x + prefix.length
    let partial: i64 -> i64 = add3 seed 2
    let functions = [plain, immediate, heap, partial]
    let copy = functions
    let mut total: i64 = 0
    for f in copy do total = total + f 10
    total + functions[1] 0
";

/// The IR of the `Mix` program for a native or a wasm32 target, emitted twice to see that it is
/// deterministic.
fn mix_ir(wasm: bool) -> String {
    let module = analyze_modules(&[("Mix.tz", MIX), ("Main.tz", "Mix.closure_mix 5")]).unwrap();
    let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
    assert_eq!(
        ir,
        llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
    );
    ir
}

/// The static descriptor `@tz.desc.<name>`, or `None`.
fn descriptor<'a>(ir: &'a str, name: &str) -> Option<&'a str> {
    let head = format!("@tz.desc.{name} = ");
    ir.lines().find(|line| line.starts_with(&head))
}

/// The suffixes of the names after `prefix` of the lines that `lines` selects.
fn names<'a>(
    ir: &'a str,
    lines: impl Fn(&str) -> bool,
    prefix: &str,
    end: &str,
) -> BTreeSet<&'a str> {
    ir.lines()
        .filter(|line| lines(line))
        .filter_map(|line| line.split(prefix).nth(1))
        .map(|rest| &rest[..rest.find(end).unwrap()])
        .collect()
}

/// A record with `count` function-valued fields.
fn handlers(count: usize) -> String {
    let fields = (0..count)
        .map(|index| format!("f{index}: i64 -> i64"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("record Handlers {{ {fields} }}\nexport def probe :: i64 -> i64\nfn probe n = n\n")
}

/// The limit on an inline value (E1010) counts a function value as 32 bytes, an upper bound of the
/// size LLVM gives it. Shrinking the value does not lower the estimate: that would change which
/// programs are accepted (PM01 D4, PM02 P2-c), so 2,048 function values still fill the limit.
#[test]
fn the_layout_limit_counts_a_function_value_as_32_bytes() {
    analyze(&handlers(2048)).expect("2048 function values fit in 64 KiB");
    let error = analyze(&handlers(2049)).expect_err("one more does not");
    assert_eq!(error.code, "E1010", "{}", error.message);
}

#[test]
fn nested_fixed_arrays_of_function_values_count_the_same_estimate() {
    let table = |rows: usize| {
        format!(
            "record Tables {{ rows: [[i64 -> i64; 1024]; {rows}] }}\nexport def probe :: i64 -> i64\nfn probe n = n\n"
        )
    };
    analyze(&table(2)).expect("2 * 1024 * 32 bytes is exactly the limit");
    let error = analyze(&table(3)).expect_err("3 * 1024 * 32 bytes is over it");
    assert_eq!(error.code, "E1010", "{}", error.message);
}

/// Every adapter `@tz.apply.*` has one static descriptor, in its own block, and the descriptor
/// starts with it: the closure of each `(function, captures)` pair is built from it.
#[test]
fn closure_descriptors_pair_with_apply_functions() {
    for wasm in [false, true] {
        let ir = mix_ir(wasm);
        let applies = names(
            &ir,
            |line| line.starts_with("define internal "),
            "@tz.apply.",
            "(",
        );
        let descriptors = names(
            &ir,
            |line| line.starts_with("@tz.desc."),
            "@tz.desc.",
            " = ",
        );
        assert!(!applies.is_empty());
        assert_eq!(applies, descriptors, "wasm: {wasm}");
        for name in &descriptors {
            let head = format!(
                "@tz.desc.{name} = internal unnamed_addr constant %tz.closure.desc {{ ptr @tz.apply.{name}, "
            );
            assert!(ir.contains(&head), "{name}");
        }
        // A descriptor is a block of its own: the readers that split at blank lines find each
        // function at the start of a block.
        for block in ir.split("\n\n").filter(|block| block.contains("@tz.desc.")) {
            assert!(
                block.starts_with("@tz.desc.") || block.starts_with("define "),
                "{block}"
            );
            assert!(
                !block.starts_with("define ") || !block.contains("\n@tz.desc."),
                "{block}"
            );
        }
        assert!(ir.contains("%tz.closure.desc = type { ptr, ptr, ptr }\n"));
    }
}

/// What a descriptor names depends on the kind of closure, and on the target: a capture that
/// fits a pointer is stored in the environment slot itself, which a wasm32 `i64` does not.
#[test]
fn closure_descriptors_name_what_each_kind_of_closure_needs() {
    let native = mix_ir(false);
    let wasm = mix_ir(true);
    let expect = |ir: &str, name: &str, clone: &str, drop: &str| {
        let line = descriptor(ir, name).unwrap_or_else(|| panic!("{name}\n{ir}"));
        assert!(
            line.ends_with(&format!("ptr {clone}, ptr {drop} }}")),
            "{name}: {line}"
        );
    };
    // A function used as a value owns nothing.
    expect(&native, "Mix.inc.0", "null", "null");
    expect(&wasm, "Mix.inc.0", "null", "null");
    // One captured `i64` is its own copy on a 64-bit target and a heap environment on wasm32.
    expect(
        &native,
        "Mix.add3.1",
        "@tz.closure.immediate.clone",
        "@tz.closure.immediate.drop",
    );
    expect(
        &wasm,
        "Mix.add3.1",
        "@tz.env.clone.Mix.add3.1",
        "@tz.env.drop.Mix.add3.1",
    );
    // Two captures are always a heap environment.
    for ir in [&native, &wasm] {
        expect(
            ir,
            "Mix.add3.2",
            "@tz.env.clone.Mix.add3.2",
            "@tz.env.drop.Mix.add3.2",
        );
    }
}

/// A task is never cloned, and a function that borrows owned captures is not cloned either:
/// neither descriptor names a clone function, which does not exist.
#[test]
fn tasks_and_owned_captures_have_no_clone_entry() {
    let source = "extern def drop_log :: i64 -> unit
record Resource { id: i64, name: string }
instance Drop<Resource> { fn drop value = drop_log value.id }
def make :: i64 -> Resource = \\id -> Resource { id: id, name: \"r\" }
export def owned :: i64 -> i64
fn owned n =
    let held = make n
    let add = Owned.function (\\amount -> amount + held.id)
    let waited = Task.run (task { return n + 1 })
    Owned.call (ref add) 1 + waited
";
    let module =
        analyze(source).unwrap_or_else(|error| panic!("{}: {}", error.code, error.message));
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert!(!ir.contains("@tz.env.clone.$task."), "{ir}");
        assert!(!ir.contains("@tz.env.clone.$lambda."), "{ir}");
        let mut seen = (false, false);
        for line in ir.lines().filter(|line| line.starts_with("@tz.desc.")) {
            let name = line["@tz.desc.".len()..].split(' ').next().unwrap();
            if name.starts_with("$task.") && !name.ends_with(".0") {
                seen.0 = true;
                assert!(
                    line.contains(", ptr null, ptr @tz.env.drop.$task."),
                    "{line}"
                );
            }
            if name.starts_with("$lambda.") && line.contains("@tz.env.drop.") {
                seen.1 = true;
                assert!(
                    line.contains(", ptr null, ptr @tz.env.drop.$lambda."),
                    "{line}"
                );
            }
        }
        assert_eq!(seen, (true, true), "wasm: {wasm}\n{ir}");
    }
}
