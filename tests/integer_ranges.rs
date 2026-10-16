//! PR02: integer range proofs and overflow flags. Without a proof an integer `+ - *` wraps, so no
//! instruction may carry `nsw` or `nuw`; these tests pin that baseline and the loop shapes that the
//! structural proofs (R1-R5) depend on.

use std::collections::BTreeMap;
use std::process::Command;

use tsuzuri::analyze;
use tsuzuri::llvm::{self, RangeFlags};

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

/// The IR of `source` in one flag mode, which does not depend on `TSUZURI_RANGE_FLAGS`; two
/// emissions must be identical.
fn emit_mode(source: &str, wasm: bool, flags: RangeFlags) -> String {
    let module = analyze(source).unwrap_or_else(|error| {
        panic!(
            "{source}\n{}: {} at {:?}",
            error.code, error.message, error.span
        )
    });
    let options = llvm::EmitOptions {
        entry: llvm::Entry::Library,
        wasm,
        wasm_simd128: false,
        debug_output: false,
        runtime_objects: false,
        allocator: llvm::Allocator::System,
        f16: llvm::F16Path::Soft,
    };
    let first = llvm::emit_with_range_flags(&module, options, flags).unwrap();
    assert_eq!(
        first,
        llvm::emit_with_range_flags(&module, options, flags).unwrap()
    );
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
        // The range loop and the loop that fills the new array.
        ("counted", 2),
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
        assert!(function_ir(&ir, "partial").contains("@tz.apply.$intrinsic.Add.add"));
        assert!(function_ir(&ir, "value").contains("@tz.apply.$intrinsic.Add.add"));
        assert!(function_ir(&ir, "join").contains("call "));
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
