use tsuzuri::{analyze, check::Type, llvm};

#[test]
fn integer_builtin_signatures_and_constraints() {
    for (source, expected) in [
        ("Int.count_ones 255i8u", Type::I64),
        ("Int.leading_zeros 0i128", Type::I64),
        ("Int.rotate_left 1i32 33", Type::Integer(32, true)),
        ("Int.unsigned_abs (-128i8)", Type::Integer(8, false)),
        ("Int.abs_diff (-128i8) 127i8", Type::Integer(8, false)),
        ("Int.widening_mul 2i64 3i64", Type::Integer(128, true)),
        ("Int.saturating_mul 100i8 100i8", Type::Integer(8, true)),
    ] {
        let module = analyze(source).unwrap();
        assert_eq!(
            module.functions[module.entry.unwrap()].signature.result,
            expected
        );
    }
    analyze("def count :: Integer<'a> => 'a -> i64\nfn count value = Int.count_ones value")
        .unwrap();
    for (source, code) in [
        ("Int.is_power_of_two 1i64", "E1005"),
        ("Int.unsigned_abs 1i64u", "E1005"),
        ("Int.widening_mul 1i128 2i128", "E1005"),
        ("Int.count_ones 1.0", "E1005"),
        ("instance UnsignedInteger<i64> {}", "E1016"),
        (
            "def absolute :: SignedInteger<'a> => 'a -> 'b\nfn absolute value = Int.unsigned_abs value",
            "E1015",
        ),
    ] {
        let error = analyze(source).expect_err(source);
        assert_eq!(error.code, code, "{source}\n{}", error.message);
    }
}

/// The bit and selection builtins, written over `left` and `right`.
const SELECTION_EXPRESSIONS: [&str; 11] = [
    "Int.min left right",
    "Int.max left right",
    "Int.clamp left right right",
    "Int.count_ones left",
    "Int.leading_zeros left",
    "Int.trailing_zeros left",
    "Int.rotate_left left (-1)",
    "Int.rotate_right left 65",
    "Int.swap_bytes left",
    "Int.reverse_bits left",
    "Int.abs_diff left right",
];

#[test]
fn integer_bit_and_selection_intrinsics_are_typed_and_deterministic() {
    for bits in [8, 16, 32, 64, 128] {
        for suffix in ["", "u"] {
            let ty = format!("i{bits}{suffix}");
            for expression in SELECTION_EXPRESSIONS {
                let module = analyze(&format!(
                    "def run :: {ty} -> {ty} -> i64\nfn run left right = ({expression}) as i64"
                ))
                .unwrap();
                for wasm in [false, true] {
                    let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
                    assert_eq!(
                        ir,
                        llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
                    );
                    if expression.contains("zeros") {
                        assert!(ir.contains("i1 false)"));
                    }
                }
            }
        }
    }
}

/// Number formatting links `numeric.ll`, which declares `llvm.smin.i32`, `llvm.smax.i32`, and
/// `llvm.ctlz.i32` itself. The generated code declares the same intrinsics for `i32` and `i32u`;
/// LLVM rejects the second `declare` even when only its attribute group differs (D12).
#[test]
fn integer_intrinsics_beside_number_formatting_declare_each_symbol_once() {
    for bits in [8, 16, 32, 64, 128] {
        for suffix in ["", "u"] {
            let ty = format!("i{bits}{suffix}");
            // One module per type holds every expression: its declarations are the union of the
            // expressions' and meet the runtime's once, and the type checker runs once.
            let library: String = SELECTION_EXPRESSIONS
                .iter()
                .enumerate()
                .map(|(index, expression)| {
                    format!(
                        "def run{index} :: {ty} -> {ty} -> string\nfn run{index} left right = to_string (({expression}) as i64)\n"
                    )
                })
                .collect();
            // A float is shown by the numeric runtime, which declares the same intrinsics as the
            // integer expressions; integers alone are shown by the integer formatter (D2).
            let library =
                format!("{library}def shown :: f64 -> string\nfn shown x = to_string x\n");
            // The top-level result is shown through `@tz_soft_format` in the console entry.
            let total = SELECTION_EXPRESSIONS
                .iter()
                .map(|expression| format!("(({expression}) as i64)"))
                .collect::<Vec<_>>()
                .join(" + ");
            let console = format!("let left: {ty} = 5\nlet right: {ty} = 9\n({total}) as f64\n");
            let programs = [
                (llvm::Entry::Library, analyze(&library).unwrap()),
                (llvm::Entry::Console, analyze(&console).unwrap()),
            ];
            for (entry, module) in &programs {
                for wasm in [false, true] {
                    let ir = llvm::emit_target(module, *entry, wasm).unwrap();
                    assert_eq!(ir, llvm::emit_target(module, *entry, wasm).unwrap());
                    assert!(
                        ir.contains("define weak hidden i32 @tz_soft_format("),
                        "{ty}: {entry:?}: numeric.ll is linked"
                    );
                    assert_eq!(
                        llvm::duplicate_declarations(&ir),
                        Vec::<String>::new(),
                        "{ty}: {entry:?} wasm={wasm}"
                    );
                }
            }
        }
    }
}

#[test]
fn checked_saturating_and_power_lowering_preserves_maybe_layout() {
    for bits in [8, 16, 32, 64, 128] {
        for suffix in ["", "u"] {
            let ty = format!("i{bits}{suffix}");
            for operation in [
                "checked_add",
                "checked_sub",
                "checked_mul",
                "checked_div",
                "checked_rem",
                "checked_pow",
                "saturating_add",
                "saturating_sub",
                "saturating_mul",
                "wrapping_pow",
            ] {
                let right = if operation.ends_with("pow") {
                    "3"
                } else {
                    "right"
                };
                let expression = format!("Int.{operation} left {right}");
                let body = if operation.starts_with("checked_") {
                    format!("match {expression} with | Some value -> value as i64 | None -> 0")
                } else {
                    format!("({expression}) as i64")
                };
                let module = analyze(&format!(
                    "def run :: {ty} -> {ty} -> i64\nfn run left right = {body}"
                ))
                .unwrap();
                for wasm in [false, true] {
                    let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
                    assert_eq!(
                        ir,
                        llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
                    );
                    assert!(!ir.contains("mul.with.overflow.i128"));
                }
            }
        }
    }
}
