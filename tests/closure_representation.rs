//! Function values (PM02): the shape of their IR and the size model behind the layout limit.
use std::collections::BTreeSet;
use tsuzuri::driver::{BuildOptions, Emit, OptLevel, Project, Target};
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

/// A function value is a descriptor and an environment (PM02): it is built from `@tz.desc.*` and
/// its code is the first entry of the descriptor, which every reader loads right after taking the
/// descriptor out of the value. No instruction names a third or fourth field.
#[test]
fn closure_values_are_a_descriptor_and_an_environment() {
    for wasm in [false, true] {
        let ir = mix_ir(wasm);
        assert!(ir.contains("\n%tz.closure = type { ptr, ptr }\n"), "{ir}");
        let lines: Vec<&str> = ir.lines().collect();
        let (mut built, mut read) = (0, 0);
        for (position, line) in lines.iter().enumerate() {
            let line = line.trim();
            if line.contains("insertvalue %tz.closure ")
                || line.contains("extractvalue %tz.closure ")
            {
                let index: usize = line.rsplit(", ").next().unwrap().parse().unwrap();
                assert!(index < 2, "{line}");
            }
            if let Some((_, value)) =
                line.split_once("insertvalue %tz.closure zeroinitializer, ptr ")
            {
                let target = value.split(',').next().unwrap();
                let head = format!("{target} = internal unnamed_addr constant %tz.closure.desc ");
                assert!(ir.lines().any(|line| line.starts_with(&head)), "{line}");
                assert!(line.ends_with(", 0"), "{line}");
                built += 1;
            }
            if let Some((result, _)) = line.split_once(" = extractvalue %tz.closure ")
                && line.ends_with(", 0")
            {
                let next = lines[position + 1].trim();
                assert!(
                    next.ends_with(&format!(" = load ptr, ptr {result}"))
                        || next.contains(&format!(
                            " = getelementptr inbounds %tz.closure.desc, ptr {result}, i32 0, i32 "
                        )),
                    "{line}\n{next}"
                );
                read += 1;
            }
        }
        assert!(
            built > 0 && read > 0,
            "wasm: {wasm}: {built} built, {read} read"
        );
    }
}

/// A descriptor keeps its adapter alive, which takes away LLVM's bonus for inlining the last call of a
/// local function; so that a known closure still folds, native code inlines the adapter and the worker
/// of a lambda into the call (PM02). Specialized workers, tasks, and ordinary functions are left to
/// LLVM, and so is every function on WebAssembly, where the attribute made programs 30 to 50 % larger.
/// A worker that several calls name is left to LLVM too (`a_lambda_that_several_calls_name_...`).
#[test]
fn native_adapters_and_lambda_workers_inline_into_known_calls() {
    for wasm in [false, true] {
        let ir = mix_ir(wasm);
        let defined = |marker: &'static str| {
            ir.lines()
                .filter(move |line| line.starts_with("define internal ") && line.contains(marker))
                .collect::<Vec<_>>()
        };
        let adapters = defined("@tz.apply.");
        let lambdas = defined("@tz.fn.$lambda.");
        assert!(adapters.len() > 3 && lambdas.len() >= 2, "wasm: {wasm}");
        for line in adapters {
            assert_eq!(line.contains(" nounwind alwaysinline {"), !wasm, "{line}");
        }
        for line in lambdas {
            assert_eq!(line.contains(" alwaysinline"), !wasm, "{line}");
        }
        for line in defined("@tz.fn.Mix.") {
            assert!(!line.contains("alwaysinline"), "{line}");
        }
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
    // One captured `i64` is its own copy on a 64-bit target, which owns nothing, and a heap
    // environment on wasm32.
    expect(&native, "Mix.add3.1", "null", "null");
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

/// The number of closures built with a capture stored in the environment slot itself, and with an
/// environment on the heap, in `ir`. A closure whose descriptor has no drop entry is left alone by
/// `@tz.closure.clone` and `@tz.closure.drop` (PM02 P2-b), so it must own nothing: the only ones
/// without a drop entry are a closure with no captures (its environment is null) and a capture
/// stored in the slot.
fn closures_built(ir: &str) -> (usize, usize) {
    let lines: Vec<&str> = ir.lines().collect();
    let (mut immediate, mut owned) = (0, 0);
    for (position, line) in lines.iter().enumerate() {
        let Some((_, target)) =
            line.split_once("insertvalue %tz.closure zeroinitializer, ptr @tz.desc.")
        else {
            continue;
        };
        let name = target.split(',').next().unwrap();
        let count: usize = name.rsplit('.').next().unwrap().parse().unwrap();
        let without_drop = descriptor(ir, name).unwrap().ends_with(", ptr null }");
        let environment = lines[position + 1]
            .split(", ptr ")
            .nth(1)
            .and_then(|rest| rest.split(',').next())
            .unwrap_or_else(|| panic!("{}", lines[position + 1]));
        if count == 0 {
            assert_eq!(environment, "null", "{line}");
            assert!(without_drop, "{name}");
            continue;
        }
        let made = lines[..position]
            .iter()
            .rev()
            .take_while(|line| !line.starts_with("define "))
            .find(|line| line.trim_start().starts_with(&format!("{environment} = ")))
            .unwrap_or_else(|| panic!("{environment} in {line}"));
        let stored_in_the_slot = made.contains(" = inttoptr ");
        assert_eq!(stored_in_the_slot, without_drop, "{name}: {made}");
        if without_drop {
            immediate += 1;
        } else {
            owned += 1;
        }
    }
    (immediate, owned)
}

#[test]
fn only_immediate_closures_have_an_environment_without_a_drop_entry() {
    for wasm in [false, true] {
        let (immediate, owned) = closures_built(&mix_ir(wasm));
        assert!(owned > 0, "wasm: {wasm}");
        // A 64-bit capture fits the environment slot of a native pointer, not of wasm32's.
        assert_eq!(immediate > 0, !wasm, "wasm: {wasm}");
    }
}

/// An option of a function value stores no tag (PM02): it is the function value, `None` is the zero
/// value, and the case is read from the descriptor, which only `None` leaves null. This holds on
/// every target, because a descriptor is static data and never at address 0.
#[test]
fn an_option_of_a_function_value_is_the_function_value() {
    let source = "export def optional :: i64 -> i64
fn optional n =
    let callback: Maybe<i64 -> i64> = if n > 0 then Maybe.Some (\\x -> x + n) else Maybe.None
    match callback with
    | Some f -> f 1
    | None -> -1
";
    let module = analyze(source).unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        let body = ir
            .split("\n\n")
            .flat_map(|block| block.split("\n}\n"))
            .find(|block| block.contains("define internal i64 @tz.fn.Main.optional("))
            .unwrap();
        assert!(
            body.contains("phi %tz.closure [") && body.contains("[ zeroinitializer, %"),
            "wasm: {wasm}\n{body}"
        );
        assert!(
            body.contains(" = icmp eq ptr ") && body.contains(", null"),
            "{body}"
        );
        assert!(!body.contains("tz.union"), "{body}");
        assert!(
            !ir.contains("tz.union.Maybe.Maybe[fn["),
            "no union type of its own"
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
        let (_, owned) = closures_built(&ir);
        assert!(owned > 0, "wasm: {wasm}");
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

/// A lambda without captures and a body that LLVM cannot fold (each step divides by the argument),
/// called from `calls` places: a known closure of that kind is called by name.
fn many_calls(calls: usize) -> String {
    let mut body = String::from("    let f: i64 -> i64 = \\x ->\n        let v0 = x * 3 + 1\n");
    for index in 1..40 {
        body += &format!(
            "        let v{index} = v{} / (x + {}) + v{} % (x + {})\n",
            index - 1,
            index + 1,
            index - 1,
            index + 4
        );
    }
    body += "        v39\n";
    let sum = (0..calls)
        .map(|call| format!("f (seed + {call})"))
        .collect::<Vec<_>>()
        .join(" + ");
    format!("export def many :: i64 -> i64\nfn many seed =\n{body}    {sum}\n")
}

/// The `define` lines of the closure functions: adapters and the workers of lambdas.
fn closure_definitions(ir: &str) -> Vec<&str> {
    ir.lines()
        .filter(|line| {
            line.starts_with("define internal ")
                && (line.contains("@tz.apply.") || line.contains("@tz.fn.$lambda."))
        })
        .collect()
}

/// `alwaysinline` copies the body of a worker into every call that names it. A lambda that the program
/// calls from several places keeps its worker out of that rule, so that the code does not grow with the
/// number of calls; the adapters, which hold no body, keep it.
#[test]
fn a_lambda_that_several_calls_name_is_not_forced_inline() {
    for wasm in [false, true] {
        for (calls, forced) in [(1, true), (2, false), (24, false)] {
            let module = analyze(&many_calls(calls))
                .unwrap_or_else(|error| panic!("{}: {}", error.code, error.message));
            let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
            let definitions = closure_definitions(&ir);
            let (workers, adapters): (Vec<&str>, Vec<&str>) = definitions
                .into_iter()
                .partition(|line| line.contains("@tz.fn.$lambda."));
            let context = format!("wasm: {wasm}, calls: {calls}");
            assert_eq!(workers.len(), 1, "{context}\n{ir}");
            assert!(!adapters.is_empty(), "{context}");
            assert_eq!(
                workers[0].contains(" alwaysinline"),
                forced && !wasm,
                "{context}: {}",
                workers[0]
            );
            for adapter in adapters {
                assert_eq!(
                    adapter.contains(" alwaysinline"),
                    !wasm,
                    "{context}: {adapter}"
                );
            }
        }
    }
}

fn scratch(name: &str) -> std::path::PathBuf {
    let directory =
        std::env::temp_dir().join(format!("tsuzuri-closure-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

fn build_with(
    project: &Project,
    output: &std::path::Path,
    target: Target,
    emit: Emit,
    level: OptLevel,
) {
    let module = project.analyze().unwrap();
    tsuzuri::driver::build(
        &module,
        project,
        output,
        BuildOptions {
            target,
            emit,
            optimization: level,
            cache: false,
            ..BuildOptions::default()
        },
    )
    .unwrap_or_else(|error| panic!("{}: {}", error.code, error.message));
}

/// The attribute belongs to the speed levels `-O1` to `-O3` of a native build: `-O0` and the size
/// levels leave the inlining of known closures to the compiler (PM02), and WebAssembly never has it.
#[test]
fn closures_are_forced_inline_at_the_speed_levels_of_a_native_build_only() {
    let directory = scratch("levels");
    std::fs::write(directory.join("Main.tz"), many_calls(2)).unwrap();
    let project = Project::load(&directory).unwrap();
    for target in [Target::Native, Target::Wasm32] {
        for level in [
            OptLevel::O0,
            OptLevel::O1,
            OptLevel::O2,
            OptLevel::O3,
            OptLevel::Os,
            OptLevel::Oz,
        ] {
            let output = directory.join("levels.ll");
            build_with(&project, &output, target, Emit::Llvm, level);
            let ir = std::fs::read_to_string(&output).unwrap();
            let definitions = closure_definitions(&ir);
            assert!(definitions.len() >= 2, "{ir}");
            let forced = definitions
                .iter()
                .filter(|line| line.contains(" alwaysinline"))
                .count();
            let expected = target == Target::Native
                && matches!(level, OptLevel::O1 | OptLevel::O2 | OptLevel::O3);
            // The adapters keep it; the worker of this lambda has two calls and loses it.
            assert_eq!(
                forced,
                if expected { definitions.len() - 1 } else { 0 },
                "{target:?} {level:?}\n{definitions:#?}"
            );
        }
    }
    std::fs::remove_dir_all(&directory).unwrap();
}

/// What the attribute cost: a lambda of 40 steps called from 24 places was 24 copies of the body, 4.5 times
/// the code at `-O3` and `-Oz` and 16 times at `-O0`. The object must grow with the calls only, whichever
/// the level.
#[test]
fn the_object_of_a_lambda_called_from_many_places_grows_with_the_calls_only() {
    let directory = scratch("growth");
    let mut sizes = Vec::new();
    for (calls, name) in [(2, "few"), (24, "many")] {
        let source = directory.join(name);
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("Main.tz"), many_calls(calls)).unwrap();
        let project = Project::load(&source).unwrap();
        for level in [OptLevel::O0, OptLevel::O3, OptLevel::Oz] {
            let object = source.join(format!("{level:?}.o"));
            build_with(&project, &object, Target::Native, Emit::Object, level);
            sizes.push((calls, level, std::fs::metadata(&object).unwrap().len()));
        }
    }
    for level in [OptLevel::O0, OptLevel::O3, OptLevel::Oz] {
        let size = |calls: usize| {
            sizes
                .iter()
                .find(|(c, l, _)| *c == calls && *l == level)
                .unwrap()
                .2
        };
        // Twenty-two more calls cost a call and its relocation each, not a copy of the body.
        assert!(
            size(24) < size(2) + 4096,
            "{level:?}: {} bytes with 2 calls, {} with 24",
            size(2),
            size(24)
        );
    }
    std::fs::remove_dir_all(&directory).unwrap();
}
