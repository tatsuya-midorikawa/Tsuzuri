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

#[test]
fn integer_bit_and_selection_intrinsics_are_typed_and_deterministic() {
    for bits in [8, 16, 32, 64, 128] {
        for suffix in ["", "u"] {
            let ty = format!("i{bits}{suffix}");
            for expression in [
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
            ] {
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

#[test]
fn checked_saturating_and_power_lowering_preserves_option_layout() {
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
