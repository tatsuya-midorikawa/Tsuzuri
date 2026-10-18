//! PR02: integer range proofs and overflow flags. Without a proof an integer `+ - *` wraps, so no
//! instruction may carry `nsw` or `nuw`; these tests pin that baseline and the loop shapes that the
//! structural proofs (R1-R5) depend on.

use std::collections::BTreeMap;
use std::process::Command;

use tsuzuri::check::CheckedModule;
use tsuzuri::llvm::{self, RangeFlags};
use tsuzuri::{analyze, analyze_modules};

/// The IR of `source` for native or WASM; two emissions must be identical.
fn emit(source: &str, wasm: bool) -> String {
    let module = analyze(source).unwrap_or_else(|error| {
        panic!(
            "{source}\n{}: {} at {:?}",
            error.code, error.message, error.span
        )
    });
    let first = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
    assert_eq!(
        first,
        llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
    );
    first
}

/// The body of `@tz.fn.Main.<name>`, from its `define` line to the closing brace.
fn function_ir(ir: &str, name: &str) -> String {
    let marker = format!("@tz.fn.Main.{name}(");
    let rest = ir
        .split_once(&marker)
        .unwrap_or_else(|| panic!("no function {name}"))
        .1;
    rest.split("\n}").next().unwrap().to_owned()
}

/// The IR of `source`, emitted once.
fn emit_once(source: &str, wasm: bool) -> String {
    let module = analyze(source).unwrap_or_else(|error| {
        panic!(
            "{source}\n{}: {} at {:?}",
            error.code, error.message, error.span
        )
    });
    llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
}

/// The IR of `source` in one flag mode, which does not depend on `TSUZURI_RANGE_FLAGS`.
fn emit_mode_once(source: &str, wasm: bool, flags: RangeFlags) -> String {
    let module = analyze(source).unwrap_or_else(|error| {
        panic!(
            "{source}\n{}: {} at {:?}",
            error.code, error.message, error.span
        )
    });
    emit_module_mode(&module, wasm, flags)
}

/// The IR of an analyzed program in one flag mode.
fn emit_module_mode(module: &CheckedModule, wasm: bool, flags: RangeFlags) -> String {
    let options = llvm::EmitOptions {
        entry: llvm::Entry::Library,
        wasm,
        wasm_simd128: false,
        debug_output: false,
        runtime_objects: false,
        allocator: llvm::Allocator::System,
        f16: llvm::F16Path::Soft,
    };
    llvm::emit_with_range_flags(module, options, flags).unwrap()
}

/// `emit_mode_once`, which must give the same IR twice.
fn emit_mode(source: &str, wasm: bool, flags: RangeFlags) -> String {
    let first = emit_mode_once(source, wasm, flags);
    assert_eq!(first, emit_mode_once(source, wasm, flags));
    first
}

fn has_flag(text: &str) -> bool {
    text.split(|c: char| !c.is_alphanumeric())
        .any(|word| matches!(word, "nsw" | "nuw"))
}

/// One program per loop form and per kind of integer arithmetic, each with its function name.
const PROGRAMS: &[(&str, &str)] = &[
    (
        "ascending",
        "def ascending :: i64 -> i64\nfn ascending n =\n    let mut total = 0i64\n    for i in 0 .. n do total = total + i\n    total\n",
    ),
    (
        "descending",
        "def descending :: i64 -> i64\nfn descending n =\n    let mut total = 0i64\n    for i in n .. -1 .. 0 do total = total + i\n    total\n",
    ),
    (
        "unsigned",
        "def unsigned :: i64u -> i64u\nfn unsigned n =\n    let mut total = 0i64u\n    for i in 0i64u .. n do total = total + i\n    total\n",
    ),
    (
        "wide",
        "def wide :: i128 -> i128\nfn wide n =\n    let mut total = 0i128\n    for i in 0i128 .. n do total = total + i\n    total\n",
    ),
    (
        "narrow_up",
        "def narrow_up :: i32 -> i64\nfn narrow_up n =\n    let mut total = 0i64\n    for i = 0 to n - 1 do total = total + (i as i64)\n    total\n",
    ),
    (
        "narrow_down",
        "def narrow_down :: i32 -> i64\nfn narrow_down n =\n    let mut total = 0i64\n    for i = n downto 1 do total = total + (i as i64)\n    total\n",
    ),
    (
        "narrow_unsigned",
        "def narrow_unsigned :: i32u -> i64\nfn narrow_unsigned n =\n    let mut total = 0i64\n    for i in 0i32u .. n do total = total + (i as i64)\n    total\n",
    ),
    (
        "stepped",
        "def stepped :: i64 -> i64\nfn stepped n =\n    let mut total = 0i64\n    for i in 0 .. 2 .. n do total = total + i\n    total\n",
    ),
    (
        "each_array",
        "def each_array :: ref [i64] -> i64\nfn each_array values =\n    let mut total = 0i64\n    for value in values do total = total + value\n    total\n",
    ),
    (
        "each_list",
        "def each_list :: ref [|i64|] -> i64\nfn each_list values =\n    let mut total = 0i64\n    for value in values do total = total + value\n    total\n",
    ),
    (
        "each_text",
        "def each_text :: ref string -> i64\nfn each_text text =\n    let mut total = 0i64\n    for unit in text do total = total + (unit as i64)\n    total\n",
    ),
    (
        "arithmetic",
        "def arithmetic :: i64 -> i64 -> i64\nfn arithmetic a b = a + b * 3 - 1\n",
    ),
    (
        "counted",
        "def counted :: i64 -> i64\nfn counted count =\n    let values = new [i64](count, \\i -> i * 3)\n    let mut total = 0i64\n    for i in 0 .. values.length - 1 do total = total + values[i] + i * 4 + 3\n    total\n",
    ),
];

#[test]
fn programs_have_no_overflow_flags_without_a_proof() {
    for (name, source) in PROGRAMS {
        for wasm in [false, true] {
            let ir = emit_mode(source, wasm, RangeFlags::Off);
            let body = function_ir(&ir, name);
            assert!(!has_flag(&body), "{name} wasm={wasm}\n{body}");
        }
    }
}

/// A basic block of one function: its label and its instruction lines.
struct Block {
    label: String,
    lines: Vec<String>,
}

/// The blocks of a function body that `function_ir` returns (its first line is the rest of `define`).
fn blocks(body: &str) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    for line in body.lines().skip(1) {
        match line.strip_suffix(':') {
            Some(label) if !line.starts_with(' ') => blocks.push(Block {
                label: label.to_owned(),
                lines: Vec::new(),
            }),
            _ => blocks
                .last_mut()
                .expect("an instruction before the first label")
                .lines
                .push(line.trim().to_owned()),
        }
    }
    blocks
}

/// The labels that the terminator of `block` names.
fn successors(block: &Block) -> Vec<String> {
    block
        .lines
        .last()
        .unwrap()
        .split("label %")
        .skip(1)
        .map(|rest| {
            rest.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
                .next()
                .unwrap()
                .to_owned()
        })
        .collect()
}

fn predecessors<'a>(blocks: &'a [Block], label: &str) -> Vec<&'a Block> {
    blocks
        .iter()
        .filter(|block| successors(block).iter().any(|target| target == label))
        .collect()
}

/// An instruction line without its result name, and with every `%name` operand written as `%`.
fn shape(line: &str) -> String {
    let line = line.split_once(" = ").map_or(line, |(_, rest)| rest);
    let mut text = String::new();
    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        text.push(character);
        if character == '%' {
            while characters
                .next_if(|c| c.is_alphanumeric() || matches!(c, '_' | '.'))
                .is_some()
            {}
        }
    }
    text
}

/// The `add`, `sub` and `mul` instructions of a function, sorted, in `shape` form.
fn arithmetic(body: &str) -> Vec<String> {
    let mut lines: Vec<String> = body
        .lines()
        .map(str::trim)
        .filter(|line| {
            ["add ", "sub ", "mul "].iter().any(|opcode| {
                line.split_once(" = ")
                    .is_some_and(|(_, rest)| rest.starts_with(opcode))
            })
        })
        .map(shape)
        .collect();
    lines.sort();
    lines
}

fn count(text: &str, needle: &str) -> usize {
    text.matches(needle).count()
}

/// The flags that the loop increments of each program of `PROGRAMS` carry in proven mode (R1-R5),
/// next to the other arithmetic of the function, which carries none: no proof covers it yet.
const STRUCTURAL: &[(&str, &[&str])] = &[
    ("ascending", &["add i64 %, %", "add nsw i64 %, 1"]),
    (
        "descending",
        &["add i64 %, %", "add nsw i64 %, 18446744073709551615"],
    ),
    ("unsigned", &["add i64 %, %", "add nuw i64 %, 1"]),
    ("wide", &["add i128 %, %", "add nsw i128 %, 1"]),
    (
        "narrow_up",
        &["add i64 %, %", "add nsw i64 %, 1", "sub i32 %, 1"],
    ),
    ("narrow_down", &["add i64 %, %", "add nsw i64 %, -1"]),
    ("narrow_unsigned", &["add i64 %, %", "add nuw nsw i64 %, 1"]),
    ("stepped", &["add i64 %, %"]),
    ("each_array", &["add i64 %, %", "add nuw i64 %, 1"]),
    ("each_list", &["add i64 %, %", "add nuw i64 %, 1"]),
    ("each_text", &["add i64 %, %", "add nuw i64 %, 1"]),
    (
        "arithmetic",
        &["add i64 %, %", "mul i64 %, 3", "sub i64 %, 1"],
    ),
];

#[test]
fn proven_mode_flags_exactly_the_loop_increments_that_a_loop_shape_proves() {
    for (name, expected) in STRUCTURAL {
        let (_, source) = PROGRAMS.iter().find(|(n, _)| n == name).unwrap();
        for wasm in [false, true] {
            let body = function_ir(&emit_mode(source, wasm, RangeFlags::Proven), name);
            assert_eq!(arithmetic(&body), *expected, "{name} wasm={wasm}\n{body}");
        }
    }
}

/// `ir` without the overflow flags of `add`, `sub` and `mul`, and without the `noundef` that PR01
/// puts on the result of a function whose instructions cannot make a poison value: a flag is a
/// poison word to it, so proven mode loses that attribute on a function with a flagged loop.
fn without_flags(ir: &str) -> String {
    let mut text = ir.to_owned();
    for opcode in ["add", "sub", "mul"] {
        for flags in ["nuw nsw", "nsw", "nuw"] {
            text = text.replace(&format!(" = {opcode} {flags} "), &format!(" = {opcode} "));
        }
    }
    text.replace("define internal noundef ", "define internal ")
}

#[test]
fn proven_mode_differs_from_off_mode_only_by_flag_words() {
    for (name, source) in PROGRAMS {
        for wasm in [false, true] {
            let off = emit_mode(source, wasm, RangeFlags::Off);
            let proven = emit_mode(source, wasm, RangeFlags::Proven);
            assert!(!has_flag(&off), "{name}");
            assert_eq!(
                without_flags(&proven),
                without_flags(&off),
                "{name} wasm={wasm}"
            );
            for fact in ["llvm.assume", "!range", "nneg", "samesign", "nusw"] {
                assert_eq!(count(&proven, fact), count(&off, fact), "{name} {fact}");
            }
        }
    }
}

#[test]
fn a_flagged_loop_costs_the_pure_function_its_noundef_result() {
    // PR01 gives `noundef` to the result of a scalar function that cannot make poison, and counts a
    // flag as poison; proven mode trades that attribute for the loop flags (D13).
    let (_, source) = PROGRAMS
        .iter()
        .find(|(name, _)| *name == "ascending")
        .unwrap();
    let off = emit_mode(source, false, RangeFlags::Off);
    let proven = emit_mode(source, false, RangeFlags::Proven);
    assert!(off.contains("define internal noundef i64 @tz.fn.Main.ascending("));
    assert!(proven.contains("define internal i64 @tz.fn.Main.ascending("));
}

#[test]
fn audit_mode_traps_instead_of_writing_a_flag() {
    // Overflow checks per program: one per sense that proven mode claims.
    let checks: BTreeMap<&str, usize> = BTreeMap::from([
        ("ascending", 1),
        ("descending", 1),
        ("unsigned", 1),
        ("wide", 1),
        ("narrow_up", 1),
        ("narrow_down", 1),
        ("narrow_unsigned", 2),
        ("stepped", 0),
        ("each_array", 1),
        ("each_list", 1),
        ("each_text", 1),
        ("arithmetic", 0),
        // The range loop, the loop that fills the new array, and `values.length - 1`.
        ("counted", 3),
    ]);
    for (name, source) in PROGRAMS {
        for wasm in [false, true] {
            let off = function_ir(&emit_mode(source, wasm, RangeFlags::Off), name);
            let audit = function_ir(&emit_mode(source, wasm, RangeFlags::Audit), name);
            let overflow = count(&audit, "with.overflow") - count(&off, "with.overflow");
            assert!(!has_flag(&audit), "{name} wasm={wasm}\n{audit}");
            assert_eq!(overflow, checks[name], "{name} wasm={wasm}\n{audit}");
            assert_eq!(
                count(&audit, "call void @llvm.trap()") - count(&off, "call void @llvm.trap()"),
                checks[name],
                "{name} wasm={wasm}"
            );
        }
    }
}

#[test]
fn every_phi_names_a_predecessor_in_every_mode() {
    for mode in [RangeFlags::Off, RangeFlags::Proven, RangeFlags::Audit] {
        for (name, source) in PROGRAMS {
            let body = function_ir(&emit_mode(source, false, mode), name);
            let blocks = blocks(&body);
            for block in &blocks {
                for line in block.lines.iter().filter(|line| line.contains(" = phi ")) {
                    for incoming in line.split("[ ").skip(1) {
                        // `[ value, %label ]`
                        let label = incoming.split(" ]").next().unwrap();
                        let label = label.rsplit('%').next().unwrap();
                        assert!(
                            predecessors(&blocks, &block.label)
                                .iter()
                                .any(|predecessor| predecessor.label == label),
                            "{mode:?} {name}: {line}\n{body}"
                        );
                    }
                }
            }
        }
    }
}

/// `x op y` and the builtin call `Class.method x y` for each operator, and the types that carry it.
const OPERATORS: &[(&str, &str, &str)] = &[
    ("+", "Add.add", "add"),
    ("-", "Sub.sub", "sub"),
    ("*", "Mul.mul", "mul"),
];
const NUMBER_TYPES: &[&str] = &[
    "i8", "i16", "i32", "i64", "i128", "i8u", "i16u", "i32u", "i64u", "i128u", "f16", "f32", "f64",
    "f128", "d32", "d64", "d128",
];

/// Two functions that apply `operator` to their parameters, once as the operator and once as the builtin.
fn operator_program(ty: &str, operator: &str, builtin: &str) -> String {
    format!(
        "def written :: {ty} -> {ty} -> {ty}\nfn written a b = a {operator} b\n\
         def named :: {ty} -> {ty} -> {ty}\nfn named a b = {builtin} a b\n"
    )
}

#[test]
fn builtin_and_operator_forms_emit_the_same_instructions() {
    for (operator, builtin, instruction) in OPERATORS {
        for ty in NUMBER_TYPES {
            for wasm in [false, true] {
                let ir = emit_once(&operator_program(ty, operator, builtin), wasm);
                let operator_body = function_ir(&ir, "written");
                let builtin_body = function_ir(&ir, "named");
                assert_eq!(operator_body, builtin_body, "{ty} {builtin} wasm={wasm}");
                assert!(
                    !builtin_body.contains("$intrinsic"),
                    "{ty} {builtin}\n{builtin_body}"
                );
                // The types with a direct LLVM instruction show it; the others call the numeric runtime.
                let direct = match *ty {
                    "f32" | "f64" => Some(format!("f{instruction} ")),
                    "f16" | "f128" | "d32" | "d64" | "d128" => None,
                    _ => Some(format!("{instruction} ")),
                };
                if let Some(direct) = direct {
                    assert!(
                        builtin_body.contains(&direct),
                        "{ty} {builtin}\n{builtin_body}"
                    );
                    assert!(
                        !builtin_body.contains("call "),
                        "{ty} {builtin}\n{builtin_body}"
                    );
                }
            }
        }
    }
}

#[test]
fn builtin_forms_evaluate_their_operands_left_to_right_like_the_operator() {
    let source = "def f :: i64 -> i64\nfn f x = x + 1\ndef g :: i64 -> i64\nfn g x = x * 2\n\
        def operator :: i64 -> i64\nfn operator n = f n + g n\n\
        def builtin :: i64 -> i64\nfn builtin n = Add.add (f n) (g n)\n";
    for wasm in [false, true] {
        let ir = emit(source, wasm);
        for name in ["operator", "builtin"] {
            let body = function_ir(&ir, name);
            let first = body.find("call i64 @tz.fn.Main.f(").unwrap();
            let second = body.find("call i64 @tz.fn.Main.g(").unwrap();
            assert!(first < second, "{name}\n{body}");
            assert_eq!(body.matches("call ").count(), 2, "{name}\n{body}");
        }
    }
}

#[test]
fn builtin_calls_that_are_not_two_number_operands_stay_calls() {
    // Partial application and a function value keep the generated method; a string `Add.add` takes
    // its operands, and a function that merely has the name `Add.add` in a user module is a call.
    let source = "def partial :: i64 -> i64\nfn partial n = { let add = Add.add 20; add n }\n\
        def value :: unit -> (i64 -> i64 -> i64)\nfn value _unit = Add.add\n\
        def join :: string -> string -> string\nfn join a b = Add.add a b\n";
    for wasm in [false, true] {
        let ir = emit(source, wasm);
        assert!(function_ir(&ir, "partial").contains("@tz.desc.$intrinsic.Add.add"));
        assert!(function_ir(&ir, "value").contains("@tz.desc.$intrinsic.Add.add"));
        assert!(function_ir(&ir, "join").contains("call "));
    }
    // A user module named `Add` is not the generated one: whatever its `add` does, it is called, so
    // `Add.add 40 2` is 1042 there and not 42. A user function with the shape of the generated `x - y`
    // is called as well.
    let module = analyze_modules(&[
        (
            "Add",
            "def add :: i64 -> i64 -> i64\nfn add a b = a + b + 1000\n\
             def minus :: i64 -> i64 -> i64\nfn minus a b = a - b",
        ),
        (
            "Main",
            "def answer :: i64\nfn answer = Add.add 40 2\n\
             def shaped :: i64\nfn shaped = Add.minus 40 2",
        ),
    ])
    .unwrap();
    for wasm in [false, true] {
        for flags in [RangeFlags::Off, RangeFlags::Proven, RangeFlags::Audit] {
            let ir = emit_module_mode(&module, wasm, flags);
            for (name, callee) in [("answer", "Add.add"), ("shaped", "Add.minus")] {
                let body = function_ir(&ir, name);
                assert!(
                    body.contains(&format!("call i64 @tz.fn.{callee}(i64 40, i64 2)"))
                        && !body.contains("$intrinsic"),
                    "{flags:?} wasm={wasm}\n{body}"
                );
            }
            assert!(ir.contains(", 1000"), "{flags:?} wasm={wasm}");
        }
    }
}

/// The `[ value, %label ]` pairs of a `phi` line.
fn incoming(line: &str) -> Vec<(String, String)> {
    line.split("[ ")
        .skip(1)
        .map(|pair| {
            let (value, label) = pair.split(" ]").next().unwrap().split_once(", %").unwrap();
            (value.to_owned(), label.to_owned())
        })
        .collect()
}

/// The induction `phi` of the only loop of a function, with the blocks that the proofs rely on.
struct Counter<'a> {
    header: &'a Block,
    current: String,
    first: String,
    next: String,
    entry: &'a Block,
    increment: &'a Block,
}

fn counter<'a>(blocks: &'a [Block], ty: &str) -> Counter<'a> {
    let block = |label: &str| blocks.iter().find(|block| block.label == label).unwrap();
    let phis: Vec<(&Block, &String)> = blocks
        .iter()
        .flat_map(|block| block.lines.iter().map(move |line| (block, line)))
        .filter(|(_, line)| line.contains(&format!(" = phi {ty} [ ")) && incoming(line).len() == 2)
        .collect();
    // The index of a list loop comes first; a function of these tests has one loop.
    let (header, line) = phis[0];
    let [(first, entry), (next, increment)] = incoming(line).try_into().unwrap();
    Counter {
        header,
        current: line.split(" = ").next().unwrap().to_owned(),
        first,
        next,
        entry: block(&entry),
        increment: block(&increment),
    }
}

/// The two last lines of a block: a comparison and the branch on it.
fn tested(block: &Block) -> (&str, &str) {
    let n = block.lines.len();
    (&block.lines[n - 2], &block.lines[n - 1])
}

/// R1: a loop of step 1 or -1 over a type of at least 64 bits.
fn assert_range_loop(body: &str, ty: &str, order: &str, stride: &str) {
    let blocks = blocks(body);
    let counter = counter(&blocks, ty);
    let (current, next) = (&counter.current, &counter.next);
    // The increment is the first instruction of a block that jumps back to the header.
    assert_eq!(counter.increment.lines.len(), 2, "{body}");
    assert_eq!(
        counter.increment.lines[0],
        format!("{next} = add {ty} {current}, {stride}"),
        "{body}"
    );
    assert!(counter.increment.lines[1].starts_with(&format!("br label %{}", counter.header.label)));
    // Its only predecessor leaves the loop when `current == last`, so `current != last` holds here.
    let latches = predecessors(&blocks, &counter.increment.label);
    assert_eq!(latches.len(), 1, "{body}");
    let (compare, branch) = tested(latches[0]);
    let (done, operands) = compare.split_once(" = icmp eq ").unwrap();
    let last = operands.split_once(", ").unwrap().1;
    assert_eq!(operands, format!("{ty} {current}, {last}"), "{body}");
    assert!(
        branch.starts_with(&format!("br i1 {done}, label %"))
            && branch.ends_with(&format!(", label %{}", counter.increment.label)),
        "{branch}\n{body}"
    );
    // `first` is on the right side of `last`, or the loop is not entered.
    let (compare, branch) = tested(counter.entry);
    assert_eq!(
        compare.split_once(" = ").unwrap().1,
        format!("icmp {order} {ty} {}, {last}", counter.first),
        "{body}"
    );
    assert!(
        branch.contains(&format!(", label %{}, label %", counter.header.label)),
        "{branch}\n{body}"
    );
}

/// R2: a loop of step 1 or -1 over a type of fewer than 64 bits runs on the extended i64 values.
fn assert_narrow_loop(body: &str, extension: &str, order: &str, stride: &str) {
    let blocks = blocks(body);
    let counter = counter(&blocks, "i64");
    let (current, next) = (&counter.current, &counter.next);
    assert!(
        counter.increment.lines[0] == format!("{next} = add i64 {current}, {stride}")
            && counter.increment.lines[1]
                .starts_with(&format!("br label %{}", counter.header.label)),
        "{body}"
    );
    // The header tests `current` against the extended last value before anything uses it.
    let (compare, branch) = tested(counter.header);
    let (_, operands) = compare.split_once(" = icmp ").unwrap();
    let (compared, last) = operands.split_once(", ").unwrap();
    assert_eq!(compared, format!("{order} i64 {current}"), "{body}");
    assert!(branch.starts_with("br i1 "), "{body}");
    for endpoint in [counter.first.as_str(), last] {
        assert!(
            counter.entry.lines.iter().any(|line| line
                .starts_with(&format!("{endpoint} = {extension} i"))
                && line.ends_with(" to i64")),
            "{endpoint}\n{body}"
        );
    }
}

/// R3, R4, R5: a loop that counts from 0 while the index is below a length, as unsigned numbers.
fn assert_index_loop(body: &str) {
    let blocks = blocks(body);
    let counter = counter(&blocks, "i64");
    let (current, next) = (&counter.current, &counter.next);
    assert_eq!(counter.first, "0", "{body}");
    let increment = &counter.increment.lines;
    let add = increment
        .iter()
        .find(|line| line.starts_with(&format!("{next} = add ")))
        .unwrap_or_else(|| panic!("{body}"));
    assert_eq!(*add, format!("{next} = add i64 {current}, 1"), "{body}");
    assert!(
        increment
            .last()
            .unwrap()
            .starts_with(&format!("br label %{}", counter.header.label)),
        "{body}"
    );
    let (compare, branch) = tested(counter.header);
    let (_, operands) = compare.split_once(" = icmp ult i64 ").unwrap();
    assert!(operands.starts_with(&format!("{current}, ")), "{body}");
    assert!(branch.starts_with("br i1 "), "{body}");
}

#[test]
fn the_loop_shapes_that_the_structural_proofs_rely_on() {
    let program = |name: &str| PROGRAMS.iter().find(|(n, _)| *n == name).unwrap().1;
    for wasm in [false, true] {
        let body = |name: &str| function_ir(&emit_mode(program(name), wasm, RangeFlags::Off), name);
        assert_range_loop(&body("ascending"), "i64", "sle", "1");
        assert_range_loop(&body("descending"), "i64", "sge", "18446744073709551615");
        assert_range_loop(&body("unsigned"), "i64", "ule", "1");
        assert_range_loop(&body("wide"), "i128", "sle", "1");
        assert_narrow_loop(&body("narrow_up"), "sext", "sle", "1");
        assert_narrow_loop(&body("narrow_down"), "sext", "sge", "-1");
        assert_narrow_loop(&body("narrow_unsigned"), "zext", "sle", "1");
        for name in ["each_array", "each_list", "each_text"] {
            assert_index_loop(&body(name));
        }
        for source in [ARRAY_COMPARE, LIST_COMPARE] {
            assert_index_loop(&compare_loop(source, wasm, RangeFlags::Off));
        }
    }
}

/// Structural comparison of arrays and of lists (R5): the generated `Eq.eq` and `Ord.lt` methods.
const ARRAY_COMPARE: &str = "def same :: [i64] -> [i64] -> bool\nfn same a b = a == b\n";
const LIST_COMPARE: &str = "def before :: [|i64|] -> [|i64|] -> bool\nfn before a b = a < b\n";

/// The bodies of the functions of `ir` whose name starts with `prefix`, in `function_ir` form.
fn functions_named(ir: &str, prefix: &str) -> Vec<String> {
    ir.split("\ndefine ")
        .skip(1)
        .filter_map(|text| {
            let (header, rest) = text.split_once('(')?;
            let name = header.rsplit('@').next()?;
            name.starts_with(prefix)
                .then(|| rest.split("\n}").next().unwrap().to_owned())
        })
        .collect()
}

/// The generated method of `source` that compares element by element up to the shorter length.
fn compare_loop(source: &str, wasm: bool, flags: RangeFlags) -> String {
    let ir = emit_mode(source, wasm, flags);
    let mut methods = functions_named(&ir, "tz.fn.$intrinsic.")
        .into_iter()
        .filter(|body| body.contains("select i1") && body.contains("icmp ult i64"));
    let method = methods.next().unwrap_or_else(|| panic!("{ir}"));
    assert!(methods.next().is_none(), "{ir}");
    method
}

#[test]
fn structural_comparison_loops_count_with_an_unsigned_index() {
    for source in [ARRAY_COMPARE, LIST_COMPARE] {
        for wasm in [false, true] {
            let off = compare_loop(source, wasm, RangeFlags::Off);
            let proven = compare_loop(source, wasm, RangeFlags::Proven);
            let audit = compare_loop(source, wasm, RangeFlags::Audit);
            assert_eq!(arithmetic(&off), ["add i64 %, 1"], "{off}");
            assert_eq!(arithmetic(&proven), ["add nuw i64 %, 1"], "{proven}");
            assert_eq!(without_flags(&proven), without_flags(&off));
            assert!(!has_flag(&audit), "{audit}");
            assert_eq!(count(&audit, "@llvm.uadd.with.overflow.i64("), 1, "{audit}");
            assert_eq!(count(&audit, "@llvm.sadd.with.overflow"), 0, "{audit}");
        }
    }
}

#[test]
fn the_flag_modes_have_names_and_the_default_writes_no_flag() {
    assert_eq!(RangeFlags::DEFAULT, RangeFlags::Off);
    assert_eq!(RangeFlags::default(), RangeFlags::DEFAULT);
    for mode in [RangeFlags::Off, RangeFlags::Proven, RangeFlags::Audit] {
        assert_eq!(RangeFlags::parse(mode.name()), Some(mode));
    }
    for name in ["", "on", "Proven", "off ", "true", "1"] {
        assert_eq!(RangeFlags::parse(name), None, "{name:?}");
    }
}

#[test]
fn the_environment_chooses_the_mode_and_a_wrong_name_is_an_error() {
    let root = std::env::temp_dir().join(format!("tsuzuri-range-flags-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Main.tz"), PROGRAMS[0].1).unwrap();
    let build = |value: Option<&str>| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tsuzuri"));
        command
            .args(["build", "--emit", "llvm", "-O0", "-o"])
            .arg(root.join("out.ll"))
            .arg(root.join("Main.tz"))
            .env("TSUZURI_CACHE_DIR", root.join("cache"))
            .env_remove("TSUZURI_RANGE_FLAGS");
        if let Some(value) = value {
            command.env("TSUZURI_RANGE_FLAGS", value);
        }
        let output = command.output().unwrap();
        let ir = std::fs::read_to_string(root.join("out.ll")).ok();
        let _ = std::fs::remove_file(root.join("out.ll"));
        (output, ir)
    };
    // One cache directory serves every build: the mode belongs to the cache key.
    for (value, flags, checks) in [
        (None, false, false),
        (Some("proven"), true, false),
        (Some("off"), false, false),
        (Some(""), false, false),
        (Some("audit"), false, true),
        (Some("proven"), true, false),
        (None, false, false),
    ] {
        let (output, ir) = build(value);
        assert!(output.status.success(), "{value:?}");
        let ir = ir.unwrap();
        assert_eq!(has_flag(&ir), flags, "{value:?}");
        assert_eq!(ir.contains("with.overflow"), checks, "{value:?}");
    }
    let (output, ir) = build(Some("fast"));
    assert_eq!(output.status.code(), Some(1));
    assert!(ir.is_none());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("E2000")
            && stderr.contains("TSUZURI_RANGE_FLAGS must be off, proven or audit, not 'fast'"),
        "{stderr}"
    );
    std::fs::remove_dir_all(&root).unwrap();
}

// The flags that the intervals prove, checked against an evaluation of every input that does not
// use the compiler: each case lists the operands of its `+ - *` for one input, and the flags that
// the compiler must write are those that no evaluated pair of operands overflows.

type Operands = Box<dyn Fn(i128, i128) -> Vec<Vec<(i128, i128)>>>;

/// A straight-line program of numbers of `bits` bits, with an evaluation of it.
struct Case {
    /// The program, which defines `f`.
    source: &'static str,
    bits: u32,
    signed: bool,
    /// The inputs `(x, y)` that are evaluated; a program of one parameter ignores `y`.
    inputs: Vec<(i128, i128)>,
    /// The operators of the `+ - *` of `f`, in the order that they are evaluated.
    operators: &'static str,
    /// For one input, the operands of each of them in each execution, in that order.
    operands: Operands,
    /// The flags that proven mode writes on each, `""`, `"nsw"`, `"nuw"` or `"nuw nsw"`.
    expected: &'static [&'static str],
    /// Whether they are all the flags that the evaluation allows, or only some of them.
    complete: bool,
}

/// The value of the type of `bits` bits that has the bit pattern of `value`.
fn wrap(bits: u32, signed: bool, value: i128) -> i128 {
    let modulus = 1_i128 << bits;
    let pattern = value.rem_euclid(modulus);
    if signed && pattern >= modulus / 2 {
        pattern - modulus
    } else {
        pattern
    }
}

/// Whether `a operator b` overflows as signed numbers and as unsigned numbers of `bits` bits.
fn overflows(operator: char, bits: u32, a: i128, b: i128) -> (bool, bool) {
    let modulus = 1_i128 << bits;
    let apply = |left: i128, right: i128| match operator {
        '+' => left.checked_add(right),
        '-' => left.checked_sub(right),
        _ => left.checked_mul(right),
    };
    let signed = apply(wrap(bits, true, a), wrap(bits, true, b));
    let unsigned = apply(wrap(bits, false, a), wrap(bits, false, b));
    (
        signed.is_none_or(|value| value < -(modulus / 2) || value >= modulus / 2),
        unsigned.is_none_or(|value| value < 0 || value >= modulus),
    )
}

fn range(bits: u32, signed: bool) -> (i128, i128) {
    let modulus = 1_i128 << bits;
    if signed {
        (-(modulus / 2), modulus / 2 - 1)
    } else {
        (0, modulus - 1)
    }
}

/// Every value of a number type, as the input of a one-parameter program.
fn every(bits: u32, signed: bool) -> Vec<(i128, i128)> {
    let (low, high) = range(bits, signed);
    (low..=high).map(|x| (x, 0)).collect()
}

/// Every pair of values of a number type.
fn every_pair(bits: u32, signed: bool) -> Vec<(i128, i128)> {
    let (low, high) = range(bits, signed);
    (low..=high)
        .flat_map(|x| (low..=high).map(move |y| (x, y)))
        .collect()
}

/// Every pair of the values near the edges of a number type, and of the thresholds that the
/// programs below compare with.
fn edge_pairs(bits: u32, signed: bool) -> Vec<(i128, i128)> {
    let (low, high) = range(bits, signed);
    let mut values: Vec<i128> = [
        low,
        low + 1,
        low / 2,
        -3,
        -2,
        -1,
        0,
        1,
        2,
        3,
        6,
        7,
        8,
        9,
        10,
        11,
        63,
        64,
        99,
        100,
        127,
        128,
        255,
        256,
        65_535,
        65_536,
        0x7FFF_FFFF,
        0x8000_0000,
        0xFFFF_FFFF,
        0x1_0000_0000,
        high / 2,
        high - 1,
        high,
    ]
    .into_iter()
    .filter(|value| (low..=high).contains(value))
    .collect();
    values.sort_unstable();
    values.dedup();
    values
        .iter()
        .flat_map(|&x| values.iter().map(move |&y| (x, y)))
        .collect()
}

fn case(
    source: &'static str,
    (bits, signed): (u32, bool),
    inputs: Vec<(i128, i128)>,
    operators: &'static str,
    expected: &'static [&'static str],
    complete: bool,
    operands: impl Fn(i128, i128) -> Vec<Vec<(i128, i128)>> + 'static,
) -> Case {
    Case {
        source,
        bits,
        signed,
        inputs,
        operators,
        operands: Box::new(operands),
        expected,
        complete,
    }
}

fn cases() -> Vec<Case> {
    vec![
        // Widening keeps the range of the operand: 255 * 3 + 1 fits an i32 either way.
        case(
            "def f :: i8u -> i32\nfn f x = (x as i32) * 3 + 1",
            (32, true),
            every(8, false),
            "*+",
            &["nuw nsw", "nuw nsw"],
            true,
            move |x, _| vec![vec![(x, 3)], vec![(x * 3, 1)]],
        ),
        // As an unsigned number a negative i8 is large, so only the signed reading proves it.
        case(
            "def f :: i8 -> i32\nfn f x = (x as i32) * 3",
            (32, true),
            every(8, true),
            "*",
            &["nsw"],
            true,
            move |x, _| vec![vec![(x, 3)]],
        ),
        case(
            "def f :: i8 -> i8\nfn f x = x + 1",
            (8, true),
            every(8, true),
            "+",
            &[""],
            true,
            move |x, _| vec![vec![(x, 1)]],
        ),
        // The literal on the left reads the other way round: `100 < x` is `x > 100`. From 101 up the
        // signed sum overflows, and the unsigned one, 154 at most, does not.
        case(
            "def f :: i8 -> i8\nfn f x = if 100 < x then x + 27 else 0",
            (8, true),
            every(8, true),
            "+",
            &["nuw"],
            true,
            move |x, _| vec![if 100 < x { vec![(x, 27)] } else { vec![] }],
        ),
        case(
            "def f :: i8 -> i8\nfn f x = if 100 <= x then x + 27 else 0",
            (8, true),
            every(8, true),
            "+",
            &["nuw"],
            true,
            move |x, _| vec![if 100 <= x { vec![(x, 27)] } else { vec![] }],
        ),
        case(
            "def f :: i8 -> i8\nfn f x = if 100 > x && 0 <= x then x + 27 else 0",
            (8, true),
            every(8, true),
            "+",
            &["nuw nsw"],
            true,
            move |x, _| {
                vec![if (0..100).contains(&x) {
                    vec![(x, 27)]
                } else {
                    vec![]
                }]
            },
        ),
        case(
            "def f :: i8 -> i8\nfn f x = if 99 >= x && 0 < x then x + 28 else 0",
            (8, true),
            every(8, true),
            "+",
            &["nuw nsw"],
            true,
            move |x, _| {
                vec![if (1..=99).contains(&x) {
                    vec![(x, 28)]
                } else {
                    vec![]
                }]
            },
        ),
        // At 127 the signed sum overflows; the unsigned one, 128, does not.
        case(
            "def f :: i8 -> i8\nfn f x = if x >= 0 then x + 1 else 0",
            (8, true),
            every(8, true),
            "+",
            &["nuw"],
            true,
            move |x, _| vec![if x >= 0 { vec![(x, 1)] } else { vec![] }],
        ),
        case(
            "def f :: i8 -> i8\nfn f x = if x >= 0 && x < 100 then x * 2 else 0",
            (8, true),
            every(8, true),
            "*",
            &["nuw"],
            true,
            move |x, _| {
                vec![if (0..100).contains(&x) {
                    vec![(x, 2)]
                } else {
                    vec![]
                }]
            },
        ),
        case(
            "def f :: i8 -> i8\nfn f x = if x >= 0 && x < 64 then x * 2 else 0",
            (8, true),
            every(8, true),
            "*",
            &["nuw nsw"],
            true,
            move |x, _| {
                vec![if (0..64).contains(&x) {
                    vec![(x, 2)]
                } else {
                    vec![]
                }]
            },
        ),
        // An unsigned `x - 1` after `x != 0`: the same bits as a signed number overflow at 128.
        case(
            "def f :: i8u -> i8u\nfn f x = if x != 0 then x - 1 else 0",
            (8, false),
            every(8, false),
            "-",
            &["nuw"],
            true,
            move |x, _| vec![if x != 0 { vec![(x, 1)] } else { vec![] }],
        ),
        case(
            "def f :: i8u -> i8u\nfn f x = x - 1",
            (8, false),
            every(8, false),
            "-",
            &[""],
            true,
            move |x, _| vec![vec![(x, 1)]],
        ),
        case(
            "def f :: i8 -> i8\nfn f x = (x & 63) * 2 + 1",
            (8, true),
            every(8, true),
            "*+",
            &["nuw nsw", "nuw nsw"],
            true,
            move |x, _| vec![vec![(x & 63, 2)], vec![((x & 63) * 2, 1)]],
        ),
        // 127 + 1 is 128: no signed overflow is proved, an unsigned one cannot happen.
        case(
            "def f :: i8 -> i8\nfn f x = (x & 127) + 1",
            (8, true),
            every(8, true),
            "+",
            &["nuw"],
            true,
            move |x, _| vec![vec![(x & 127, 1)]],
        ),
        case(
            "def f :: i8 -> i8 -> i8\nfn f x y = x + y",
            (8, true),
            every_pair(8, true),
            "+",
            &[""],
            true,
            move |x, y| vec![vec![(x, y)]],
        ),
        case(
            "def f :: i8 -> i8 -> i8\nfn f x y = x * y",
            (8, true),
            every_pair(8, true),
            "*",
            &[""],
            true,
            move |x, y| vec![vec![(x, y)]],
        ),
        case(
            "def f :: i8u -> i8u -> i8u\nfn f x y = (x >>> 4) + (y >>> 4)",
            (8, false),
            every_pair(8, false),
            "+",
            &["nuw nsw"],
            true,
            move |x, y| vec![vec![(x >> 4, y >> 4)]],
        ),
        case(
            "def f :: i8u -> i8u\nfn f x = (x >>> 1) * 2",
            (8, false),
            every(8, false),
            "*",
            &["nuw"],
            true,
            move |x, _| vec![vec![(x >> 1, 2)]],
        ),
        case(
            "def f :: i8 -> i8\nfn f x = x % 10 + 1",
            (8, true),
            every(8, true),
            "+",
            &["nsw"],
            true,
            move |x, _| vec![vec![(x % 10, 1)]],
        ),
        case(
            "def f :: i8 -> i8\nfn f x = (x / 4) + 1",
            (8, true),
            every(8, true),
            "+",
            &["nsw"],
            true,
            move |x, _| vec![vec![(x / 4, 1)]],
        ),
        case(
            "def f :: i8 -> i8\nfn f x = (-(x & 15)) + 15",
            (8, true),
            every(8, true),
            "+",
            &["nsw"],
            true,
            move |x, _| vec![vec![(-(x & 15), 15)]],
        ),
        case(
            "def f :: i16 -> i8\nfn f x = ((x & 127) as i8) + 1",
            (8, true),
            every(16, true),
            "+",
            &["nuw"],
            true,
            move |x, _| vec![vec![(x & 127, 1)]],
        ),
        case(
            "def f :: i16 -> i32\nfn f x = (x as i32) * (x as i32)",
            (32, true),
            every(16, true),
            "*",
            &["nsw"],
            true,
            move |x, _| vec![vec![(x, x)]],
        ),
        case(
            "def f :: i32 -> i32\nfn f x = (x & 65535) * (x & 65535)",
            (32, true),
            edge_pairs(32, true)
                .into_iter()
                .map(|(x, _)| (x, 0))
                .collect(),
            "*",
            &["nuw"],
            true,
            move |x, _| vec![vec![(x & 65535, x & 65535)]],
        ),
        case(
            "def f :: i64 -> i64\nfn f x = (x & 255) * 1000000 + 1",
            (64, true),
            edge_pairs(64, true)
                .into_iter()
                .map(|(x, _)| (x, 0))
                .collect(),
            "*+",
            &["nuw nsw", "nuw nsw"],
            true,
            move |x, _| vec![vec![(x & 255, 1_000_000)], vec![((x & 255) * 1_000_000, 1)]],
        ),
        // Near misses at the edge of the range: (2^32 - 1) * 2^32 is below 2^64, (2^32 - 1) *
        // (2^32 + 1) is exactly 2^64 - 1, and (2^32 - 1) * (2^32 + 2) is above.
        case(
            "def f :: i64 -> i64\nfn f x = (x & 4294967295) * 4294967296",
            (64, true),
            edge_pairs(64, true)
                .into_iter()
                .map(|(x, _)| (x, 0))
                .collect(),
            "*",
            &["nuw"],
            true,
            move |x, _| vec![vec![(x & 0xFFFF_FFFF, 4_294_967_296)]],
        ),
        case(
            "def f :: i64 -> i64\nfn f x = (x & 4294967295) * 4294967297",
            (64, true),
            edge_pairs(64, true)
                .into_iter()
                .map(|(x, _)| (x, 0))
                .collect(),
            "*",
            &["nuw"],
            true,
            move |x, _| vec![vec![(x & 0xFFFF_FFFF, 4_294_967_297)]],
        ),
        case(
            "def f :: i64 -> i64\nfn f x = (x & 4294967295) * 4294967298",
            (64, true),
            edge_pairs(64, true)
                .into_iter()
                .map(|(x, _)| (x, 0))
                .collect(),
            "*",
            &[""],
            true,
            move |x, _| vec![vec![(x & 0xFFFF_FFFF, 4_294_967_298)]],
        ),
        case(
            "def f :: i64u -> i64u\nfn f x = x % 10 + 48",
            (64, false),
            edge_pairs(64, false)
                .into_iter()
                .map(|(x, _)| (x, 0))
                .collect(),
            "+",
            &["nuw nsw"],
            true,
            move |x, _| vec![vec![(x % 10, 48)]],
        ),
        case(
            "def f :: i64 -> i64\nfn f x = if x > 0 then x - 1 else 0",
            (64, true),
            edge_pairs(64, true)
                .into_iter()
                .map(|(x, _)| (x, 0))
                .collect(),
            "-",
            &["nuw nsw"],
            true,
            move |x, _| vec![if x > 0 { vec![(x, 1)] } else { vec![] }],
        ),
        case(
            "def f :: i64 -> i64\nfn f x = if x >= 0 then x + 1 else 0",
            (64, true),
            edge_pairs(64, true)
                .into_iter()
                .map(|(x, _)| (x, 0))
                .collect(),
            "+",
            &["nuw"],
            true,
            move |x, _| vec![if x >= 0 { vec![(x, 1)] } else { vec![] }],
        ),
        case(
            "def f :: i64 -> i64 -> i64\nfn f x y = if x == y && y == 7 then x * 100 else 0",
            (64, true),
            edge_pairs(64, true),
            "*",
            &["nuw nsw"],
            true,
            move |x, y| {
                vec![if x == y && y == 7 {
                    vec![(x, 100)]
                } else {
                    vec![]
                }]
            },
        ),
        case(
            "def f :: i64 -> i64\nfn f x = if x < 100 && x >= 0 then x + 1 else 0",
            (64, true),
            edge_pairs(64, true)
                .into_iter()
                .map(|(x, _)| (x, 0))
                .collect(),
            "+",
            &["nuw nsw"],
            true,
            move |x, _| {
                vec![if (0..100).contains(&x) {
                    vec![(x, 1)]
                } else {
                    vec![]
                }]
            },
        ),
        // The else branch is not narrowed, so what the conditional yields is any number, although
        // only a non-negative one can reach the addition.
        case(
            "def f :: i64 -> i64\nfn f x = (if x < 10 then 0 else x) + 1",
            (64, true),
            edge_pairs(64, true)
                .into_iter()
                .map(|(x, _)| (x, 0))
                .collect(),
            "+",
            &[""],
            false,
            move |x, _| vec![vec![(if x < 10 { 0 } else { x }, 1)]],
        ),
        case(
            "def f :: i64 -> i64\nfn f x = if x >= 0 && x < 100 then (x & 15) + x else 0",
            (64, true),
            edge_pairs(64, true)
                .into_iter()
                .map(|(x, _)| (x, 0))
                .collect(),
            "+",
            &["nuw nsw"],
            true,
            move |x, _| {
                vec![if (0..100).contains(&x) {
                    vec![(x & 15, x)]
                } else {
                    vec![]
                }]
            },
        ),
        // A mutable local can change, so nothing is known of it, although this one never does.
        case(
            "def f :: i64 -> i64\nfn f x =\n    let mut y = x & 15\n    y * 4 + 1",
            (64, true),
            edge_pairs(64, true)
                .into_iter()
                .map(|(x, _)| (x, 0))
                .collect(),
            "*+",
            &["", ""],
            false,
            move |x, _| vec![vec![(x & 15, 4)], vec![((x & 15) * 4, 1)]],
        ),
        case(
            "def f :: i64 -> i64\nfn f x =\n    let y = x & 15\n    let z = y + 3\n    z * z",
            (64, true),
            edge_pairs(64, true)
                .into_iter()
                .map(|(x, _)| (x, 0))
                .collect(),
            "+*",
            &["nuw nsw", "nuw nsw"],
            true,
            move |x, _| vec![vec![(x & 15, 3)], vec![((x & 15) + 3, (x & 15) + 3)]],
        ),
        case(
            "def f :: i8 -> i8 -> i8\nfn f x y = (x + y) - y",
            (8, true),
            every_pair(8, true),
            "+-",
            &["", ""],
            true,
            move |x, y| vec![vec![(x, y)], vec![(wrap(8, true, x + y), y)]],
        ),
    ]
}

/// The `add`, `sub` and `mul` of a function in the order of the text, as `(opcode, flag words)`,
/// without the negations that the compiler writes as `sub 0, x`.
fn flagged_operations(body: &str) -> Vec<(String, String)> {
    body.lines()
        .filter_map(|line| line.trim().split_once(" = ").map(|(_, rest)| rest))
        .filter(|rest| {
            ["add ", "sub ", "mul "]
                .iter()
                .any(|opcode| rest.starts_with(opcode))
        })
        .filter(|rest| !rest.starts_with("sub i") || !rest.contains(" 0, "))
        .map(|rest| {
            let mut words = rest.split_whitespace();
            let opcode = words.next().unwrap().to_owned();
            let flags: Vec<&str> = words
                .take_while(|word| matches!(*word, "nuw" | "nsw"))
                .collect();
            (opcode, flags.join(" "))
        })
        .collect()
}

#[test]
fn proven_flags_are_those_of_an_evaluation_of_every_input() {
    for case in cases() {
        let source = case.source;
        // For each operation: whether none of its executions overflows as signed, as unsigned.
        let mut free = vec![(true, true); case.operators.len()];
        for &(x, y) in &case.inputs {
            let executions = (case.operands)(x, y);
            assert_eq!(executions.len(), case.operators.len(), "{source}");
            for (index, (operator, pairs)) in case.operators.chars().zip(executions).enumerate() {
                for (a, b) in pairs {
                    // The evaluation must stay in the type, as the program does.
                    let (low, high) = range(case.bits, case.signed);
                    assert!(
                        (low..=high).contains(&a) && (low..=high).contains(&b),
                        "{source}: {a} {operator} {b}"
                    );
                    let (signed, unsigned) = overflows(operator, case.bits, a, b);
                    free[index].0 &= !signed;
                    free[index].1 &= !unsigned;
                }
            }
        }
        for wasm in [false, true] {
            let proven_ir = emit_mode_once(source, wasm, RangeFlags::Proven);
            let written = flagged_operations(&function_ir(&proven_ir, "f"));
            assert_eq!(written.len(), case.operators.len(), "{source}\n{written:?}");
            let mut claimed = 0;
            for (index, (operator, ((opcode, words), expected))) in case
                .operators
                .chars()
                .zip(written.iter().zip(case.expected))
                .enumerate()
            {
                let context = format!("{source} wasm={wasm} operation {index} ({operator})");
                let instruction = match operator {
                    '+' => "add",
                    '-' => "sub",
                    _ => "mul",
                };
                assert_eq!(opcode, instruction, "{context}");
                assert_eq!(words, expected, "{context}");
                let (signed_free, unsigned_free) = free[index];
                // A flag is written only when no evaluated pair overflows in that sense ...
                assert!(
                    !words.contains("nsw") || signed_free,
                    "unsound nsw: {context}"
                );
                assert!(
                    !words.contains("nuw") || unsigned_free,
                    "unsound nuw: {context}"
                );
                // ... and, where the case says so, whenever that holds.
                if case.complete {
                    assert_eq!(words.contains("nsw"), signed_free, "missing nsw: {context}");
                    assert_eq!(
                        words.contains("nuw"),
                        unsigned_free,
                        "missing nuw: {context}"
                    );
                }
                claimed += words.split_whitespace().count();
            }
            // Audit mode checks each claimed sense, and writes no flag.
            let off_ir = emit_mode_once(source, wasm, RangeFlags::Off);
            let audit_ir = emit_mode_once(source, wasm, RangeFlags::Audit);
            let (off, audit) = (function_ir(&off_ir, "f"), function_ir(&audit_ir, "f"));
            assert!(!has_flag(&off) && !has_flag(&audit), "{source}");
            assert_eq!(
                count(&audit, "with.overflow") - count(&off, "with.overflow"),
                claimed,
                "{source} wasm={wasm}\n{audit}"
            );
            assert_eq!(
                without_flags(&proven_ir),
                without_flags(&off_ir),
                "{source} wasm={wasm}"
            );
        }
    }
}

/// Loops, and the `+ - *` of their bodies: the sorted shapes of every integer instruction of `f` in
/// proven mode. The loop counters carry the flags of their loop (R1-R5), the instructions of the
/// body those that the range of the counter proves, and a mutable local proves nothing.
const LOOPS: &[(&str, &[&str])] = &[
    (
        "def f :: i64 -> i64\nfn f n =\n    let mut total = 0\n    for i in 0 .. 99 do total = total + i * 2\n    total",
        &["add i64 %, %", "add nsw i64 %, 1", "mul nuw nsw i64 %, 2"],
    ),
    (
        "def f :: i64 -> i64\nfn f n =\n    let mut total = 0\n    for i in 100 .. -1 .. 1 do total = total + (i - 1)\n    total",
        &[
            "add i64 %, %",
            "add nsw i64 %, 18446744073709551615",
            "sub nuw nsw i64 %, 1",
        ],
    ),
    // The counter of a loop to an unknown bound can be the maximum: `i + 1` cannot be proved to be
    // free of signed overflow, but the counter is not negative.
    (
        "def f :: i64 -> i64\nfn f n =\n    let mut total = 0\n    for i in 0 .. n do total = total + (i + 1)\n    total",
        &["add i64 %, %", "add nsw i64 %, 1", "add nuw i64 %, 1"],
    ),
    (
        "def f :: i64 -> i64\nfn f n =\n    let mut total = 0\n    for i in 0 .. n do total = total + i * 2\n    total",
        &["add i64 %, %", "add nsw i64 %, 1", "mul nuw i64 %, 2"],
    ),
    (
        "def f :: i64 -> i64\nfn f n =\n    let mut total = 0\n    for i in 0 .. n do total = total + i * 3\n    total",
        &["add i64 %, %", "add nsw i64 %, 1", "mul i64 %, 3"],
    ),
    (
        "def f :: ref [i64] -> i64\nfn f values =\n    let mut total = 0\n    for i in 0 .. values.length - 1 do total = total + (i + 1)\n    total",
        &[
            "add i64 %, %",
            "add nsw i64 %, 1",
            "add nuw nsw i64 %, 1",
            "sub nsw i64 %, 1",
        ],
    ),
    // A step of unknown sign keeps the counter between the bounds, which are known.
    (
        "def f :: i64 -> i64 -> i64\nfn f n s =\n    let mut total = 0\n    for i in 0 .. s .. 9 do total = total + (i + 1)\n    total",
        &["add i64 %, %", "add nuw nsw i64 %, 1"],
    ),
    (
        "def f :: i64 -> i64\nfn f n =\n    let mut total = 0\n    for i in 9 .. -1 .. n do total = total + (i + 1)\n    total",
        &[
            "add i64 %, %",
            "add nsw i64 %, 1",
            "add nsw i64 %, 18446744073709551615",
        ],
    ),
    (
        "def f :: i8u -> i64\nfn f n =\n    let mut total = 0\n    for i in 0i8u .. n do total = total + ((i as i64) + 1)\n    total",
        &[
            "add i64 %, %",
            "add nuw nsw i64 %, 1",
            "add nuw nsw i64 %, 1",
        ],
    ),
    // A counter that the program updates is a mutable local, whatever its loop condition says.
    (
        "def f :: i64 -> i64\nfn f n =\n    let mut total = 0\n    let mut i = 0\n    while i < 10 do\n        total = total + i\n        i = i + 1\n    total",
        &["add i64 %, %", "add i64 %, 1"],
    ),
    // A descending counter takes its largest value first: `i + 1` overflows as a signed number at
    // the start, although the loop ends at 9223372036854775806.
    (
        "def f :: i64 -> i64\nfn f n =\n    let mut total = 0\n    for i in 9223372036854775807 .. -1 .. 9223372036854775806 do total = total + (i + 1)\n    total",
        &[
            "add i64 %, %",
            "add nsw i64 %, 18446744073709551615",
            "add nuw i64 %, 1",
        ],
    ),
];

#[test]
fn loop_bodies_get_the_flags_that_the_range_of_their_counter_proves() {
    for (source, expected) in LOOPS {
        for wasm in [false, true] {
            let ir = emit_mode_once(source, wasm, RangeFlags::Proven);
            assert_eq!(
                arithmetic(&function_ir(&ir, "f")),
                *expected,
                "{source} wasm={wasm}"
            );
            let off = emit_mode_once(source, wasm, RangeFlags::Off);
            assert_eq!(
                without_flags(&ir),
                without_flags(&off),
                "{source} wasm={wasm}"
            );
            let flags: usize = arithmetic(&function_ir(&ir, "f"))
                .iter()
                .map(|line| {
                    line.split_whitespace()
                        .take_while(|word| !word.starts_with('i'))
                        .filter(|word| matches!(*word, "nuw" | "nsw"))
                        .count()
                })
                .sum();
            let audit = function_ir(&emit_mode_once(source, wasm, RangeFlags::Audit), "f");
            let checks =
                count(&audit, "with.overflow") - count(&function_ir(&off, "f"), "with.overflow");
            assert_eq!(checks, flags, "{source} wasm={wasm}\n{audit}");
            assert!(!has_flag(&audit), "{source}");
        }
    }
}

#[test]
fn the_arguments_of_a_self_tail_call_get_the_flags_of_their_operands() {
    // The subtraction is deferred to the loop latch on native and written in place on WASM; both
    // carry the flag that the condition proves, and the accumulator, which is unknown, carries none.
    let source = "def rec count :: i64u -> i64u -> i64u\nfn rec count n acc = if n != 0 then count (n - 1) (acc + 3) else acc\n";
    for wasm in [false, true] {
        let proven = emit_mode(source, wasm, RangeFlags::Proven);
        let body = function_ir(&proven, "count");
        assert_eq!(
            arithmetic(&body),
            ["add i64 %, 3", "sub nuw i64 %, 1"],
            "wasm={wasm}\n{body}"
        );
        assert!(!body.contains("call i64 @tz.fn.Main.count"), "{body}");
        let off = emit_mode(source, wasm, RangeFlags::Off);
        assert_eq!(without_flags(&proven), without_flags(&off));
        let audit = function_ir(&emit_mode(source, wasm, RangeFlags::Audit), "count");
        assert!(!has_flag(&audit), "{audit}");
        assert_eq!(count(&audit, "@llvm.usub.with.overflow.i64("), 1, "{audit}");
    }
}
