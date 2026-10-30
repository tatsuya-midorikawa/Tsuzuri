//! A23 Phase 1: `Int.wrapping_add`, `wrapping_sub`, `wrapping_mul` and `wrapping_neg` name the
//! wrapping that the operators do silently. A full application is the operator's one instruction,
//! without `nsw` or `nuw`; a function value uses the builtin's body.

use tsuzuri::{analyze, check::Type, llvm};

const WIDTHS: [u16; 5] = [8, 16, 32, 64, 128];

#[test]
fn wrapping_functions_exist_for_every_width_and_sign() {
    for bits in WIDTHS {
        for signed in [true, false] {
            let ty = format!("i{bits}{}", if signed { "" } else { "u" });
            let mut functions = vec!["wrapping_add", "wrapping_sub", "wrapping_mul"];
            if signed {
                functions.push("wrapping_neg");
            }
            for function in functions {
                let source = if function == "wrapping_neg" {
                    format!("Int.{function} 5{ty}")
                } else {
                    format!("Int.{function} 5{ty} 6{ty}")
                };
                let module = analyze(&source).unwrap_or_else(|error| panic!("{source}: {error:?}"));
                assert_eq!(
                    module.functions[module.entry.unwrap()].signature.result,
                    Type::Integer(bits, signed),
                    "{source}"
                );
            }
        }
    }
    // A partial application is a function value of the same type.
    analyze("let add_one = Int.wrapping_add 1i8\nadd_one 127i8").unwrap();
    analyze("def negate :: SignedInteger<'a> => 'a -> 'a = \\value -> Int.wrapping_neg value")
        .unwrap();
}

#[test]
fn wrapping_neg_rejects_unsigned_integers() {
    for source in [
        "Int.wrapping_neg 5i32u",
        "Int.wrapping_neg 0i8u",
        "def negate :: i64u -> i64u = \\value -> Int.wrapping_neg value",
    ] {
        let error = analyze(source).expect_err(source);
        assert!(
            matches!(error.code, "E1005" | "E1015"),
            "{source}\n{}: {}",
            error.code,
            error.message
        );
    }
    let error = analyze("Int.wrapping_neg 5i32u").unwrap_err();
    assert_eq!(error.code, "E1005", "{}", error.message);
    assert!(error.message.contains("SignedInteger"), "{}", error.message);
}

#[test]
fn wrapping_functions_reject_mixed_float_and_wrong_arity_arguments() {
    for (source, code) in [
        ("Int.wrapping_add 5i32 6i8", "E1003"),
        ("Int.wrapping_add 5.0 6.0", "E1005"),
        ("Int.wrapping_mul 5.0f32 6.0f32", "E1005"),
        ("Int.wrapping_neg 5.0", "E1005"),
        ("def f :: i32 -> i32 = \\a -> Int.wrapping_add a", "E1003"),
        ("Int.wrapping_sub 1i64 2i64 3i64", "E1003"),
    ] {
        let error = analyze(source).expect_err(source);
        assert_eq!(error.code, code, "{source}\n{}", error.message);
    }
}

/// The text of the function `name` (without the `@`), from its `define` line to its closing brace.
fn function_body<'a>(ir: &'a str, name: &str) -> &'a str {
    let head = format!(" @{name}(");
    let start = ir
        .match_indices("\ndefine ")
        .map(|(at, _)| at + 1)
        .find(|at| {
            ir[*at..]
                .lines()
                .next()
                .is_some_and(|line| line.contains(&head))
        })
        .unwrap_or_else(|| panic!("no function {name}"));
    let end = start + ir[start..].find("\n}\n").expect("closing brace") + 2;
    &ir[start..end]
}

#[test]
fn wrapping_calls_are_inline_instructions() {
    for (function, instruction, unary) in [
        ("wrapping_add", "add i32 ", false),
        ("wrapping_sub", "sub i32 ", false),
        ("wrapping_mul", "mul i32 ", false),
        ("wrapping_neg", "sub i32 0, ", true),
    ] {
        let source = if unary {
            format!("export def f :: i32 -> i32 = \\a -> Int.{function} a")
        } else {
            format!("export def f :: i32 -> i32 -> i32 = \\a b -> Int.{function} a b")
        };
        let module = analyze(&source).unwrap();
        for wasm in [false, true] {
            let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
            let body = function_body(&ir, "tz.fn.Main.f");
            assert!(body.contains(instruction), "{function}:\n{body}");
            assert!(!body.contains("@tz.builtin.Int."), "{function}:\n{body}");
            assert!(
                !body.contains(" nsw") && !body.contains(" nuw"),
                "{function}:\n{body}"
            );
            assert!(
                !ir.contains(&format!("@tz.builtin.Int.{function}")),
                "{function}"
            );
            assert!(llvm::duplicate_declarations(&ir).is_empty());
        }
    }
    // A function value is a `$builtin` wrapper that applies the builtin fully, so its body is
    // the same single instruction, and no call of `@tz.builtin.Int.wrapping_*` remains.
    let module = analyze(
        "export def f :: i32 -> i32 = \\a ->\n    let g = Int.wrapping_add 1i32\n    g a\n\
         export def h :: i32 -> i32 = \\a ->\n    let n = Int.wrapping_neg\n    n a\n",
    )
    .unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        for (function, instruction) in [
            ("wrapping_add", "add i32 "),
            ("wrapping_neg", "sub i32 0, "),
        ] {
            let head = format!("@tz.fn.$builtin.Int.{function}.");
            let start = ir
                .match_indices("\ndefine ")
                .map(|(at, _)| at + 1)
                .find(|at| {
                    ir[*at..]
                        .lines()
                        .next()
                        .is_some_and(|line| line.contains(&head))
                })
                .unwrap_or_else(|| panic!("no wrapper of {function}:\n{ir}"));
            let end = start + ir[start..].find("\n}\n").unwrap();
            let body = &ir[start..end];
            assert!(body.contains(instruction), "{body}");
            assert!(!body.contains(" nsw") && !body.contains(" nuw"), "{body}");
        }
        assert!(!ir.contains("@tz.builtin.Int.wrapping_"), "{ir}");
    }
}
