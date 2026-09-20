use tsuzuri::{analyze, llvm};

#[test]
fn all_semantics_fixture_functions_lower_deterministically() {
    let source = include_str!("fixtures/Semantics.tzr");
    let module = analyze(source).unwrap();
    let first = llvm::emit(&module, llvm::Entry::Library).unwrap();
    let second = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(first, second);
    assert!(first.contains("call ptr @tz.fn.Main.select_function"));
    assert!(first.contains("@llvm.fptosi.sat.i64.f64"));
}

#[test]
fn deterministic_source_mutations_do_not_panic() {
    let originals = [
        "fn f(n: i64) -> i64 { if n == 0 { 1 } else { f(n - 1) } }",
        "record R { x: i64 } export fn f() -> i64 { let r = R { x: 1 }; r.x }",
        "fn f(x: [i64; 2]) -> i64 { let a = [true, false]; if a[0] { x[1] } else { 0 } }",
        "fn id(x: i64) -> i64 { x } fn f() -> i64 { let g: fn(i64) -> i64 = id; 42 |> g }",
    ];
    let mut seed = 0x1357_2468_u32;
    for original in originals {
        for _ in 0..500 {
            let mut bytes = original.as_bytes().to_vec();
            for _ in 0..3 {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let position = seed as usize % bytes.len();
                match seed % 3 {
                    0 => {
                        bytes.remove(position);
                    }
                    1 => {
                        bytes.insert(position, (32 + seed % 95) as u8);
                    }
                    _ => {
                        bytes[position] = (32 + seed % 95) as u8;
                    }
                }
            }
            let source = String::from_utf8(bytes).unwrap();
            if let Ok(module) = analyze(&source) {
                llvm::emit(&module, llvm::Entry::Library).unwrap();
            }
        }
    }
}
