use tsuzuri::{analyze, llvm, trap::TrapSource};

#[test]
fn only_native_i64_standard_sum_requests_cpu_dispatch() {
    let source = "export def sum :: ref [i64] -> i64\nfn sum values = Array.sum values\nexport def floating :: ref [f64] -> f64\nfn floating values = Array.sum values";
    let module = analyze(source).unwrap();
    let options = llvm::EmitOptions {
        entry: llvm::Entry::Library,
        wasm: false,
        debug_output: false,
        allocator: llvm::Allocator::System,
    };
    let sources: Vec<_> = std::iter::once(TrapSource {
        path: "Main.tz",
        text: source,
    })
    .chain(
        tsuzuri::stdlib::SOURCES
            .iter()
            .map(|(path, text)| TrapSource { path, text }),
    )
    .collect();
    let native = llvm::emit_native_build(&module, options, &sources, None, false).unwrap();
    assert!(native.ir.contains("call i64 @tsuzuri_cpu_sum_i64"));
    // The driver links `cpu.c` when a line starts with the declaration.
    assert_eq!(
        native
            .ir
            .lines()
            .filter(|line| line.starts_with("declare i64 @tsuzuri_cpu_sum_i64("))
            .count(),
        1
    );
    assert!(native.ir.contains("fadd double"));
    assert_eq!(
        native.ir,
        llvm::emit_native_build(&module, options, &sources, None, false)
            .unwrap()
            .ir
    );
    assert!(
        !llvm::emit_target(&module, llvm::Entry::Library, true)
            .unwrap()
            .contains("tsuzuri_cpu")
    );
    assert!(
        !llvm::emit(&module, llvm::Entry::Library)
            .unwrap()
            .contains("tsuzuri_cpu")
    );
    assert!(
        !llvm::emit_native_build(&module, options, &sources[..1], None, false)
            .unwrap()
            .ir
            .contains("tsuzuri_cpu")
    );
    let ordinary = analyze("export def answer :: i64\nfn answer = 42").unwrap();
    assert!(
        !llvm::emit_native_build(&ordinary, options, &sources, None, false)
            .unwrap()
            .ir
            .contains("tsuzuri_cpu")
    );
}

const INTEGERS: [&str; 8] = ["i8", "i16", "i32", "i64", "i8u", "i16u", "i32u", "i64u"];

fn options() -> llvm::EmitOptions {
    llvm::EmitOptions {
        entry: llvm::Entry::Library,
        wasm: false,
        debug_output: false,
        allocator: llvm::Allocator::System,
    }
}

/// The program's source followed by the bundled std, as a native build reads them.
fn sources(source: &str) -> Vec<TrapSource<'_>> {
    std::iter::once(TrapSource {
        path: "Main.tz",
        text: source,
    })
    .chain(
        tsuzuri::stdlib::SOURCES
            .iter()
            .map(|(path, text)| TrapSource { path, text }),
    )
    .collect()
}

/// `sum_{t}`, `min_{t}`, and `max_{t}` for the eight integer types (min and max return the value,
/// or 0 for an empty array), reached through one export: the host ABI takes only i64, f64, and
/// ubyte arrays.
fn integer_kernels() -> String {
    let mut source = String::new();
    let mut probe = String::from("export def probe :: i64\nfn probe =\n    let mut total = 0i64\n");
    for name in INTEGERS {
        source.push_str(&format!(
            "def sum_{name} :: ref [{name}] -> {name}\nfn sum_{name} values = Array.sum values\n"
        ));
        for operation in ["min", "max"] {
            source.push_str(&format!(
                "def {operation}_{name} :: ref [{name}] -> {name}\nfn {operation}_{name} values =\n    match Array.{operation} values with\n    | Maybe.Some value -> deref value\n    | Maybe.None -> 0{name}\n"
            ));
        }
        probe.push_str(&format!(
            "    let values_{name} = [1{name}, 2{name}]\n    total = total + (sum_{name} (ref values_{name}) as i64) + (min_{name} (ref values_{name}) as i64) + (max_{name} (ref values_{name}) as i64)\n"
        ));
    }
    probe.push_str("    total\n");
    source + &probe
}

#[test]
fn integer_sum_min_max_request_their_kernels() {
    let source = integer_kernels();
    let module = analyze(&source).unwrap_or_else(|error| panic!("{error:?}"));
    let sources = sources(&source);
    let ir = llvm::emit_native_build(&module, options(), &sources, None, false)
        .unwrap()
        .ir;
    let mut previous = 0;
    for (symbol, result) in llvm::CPU_KERNELS {
        let declaration = format!("declare {result} @{symbol}(ptr, i64)");
        assert_eq!(ir.matches(&declaration).count(), 1, "{declaration}");
        let position = ir.find(&declaration).unwrap();
        assert!(position > previous, "{symbol} is declared in table order");
        previous = position;
        // The signed and unsigned sums share a kernel.
        let calls = if symbol.contains("_sum_") { 2 } else { 1 };
        assert_eq!(
            ir.matches(&format!("call {result} @{symbol}(")).count(),
            calls,
            "{symbol}"
        );
    }
    assert_eq!(
        ir,
        llvm::emit_native_build(&module, options(), &sources, None, false)
            .unwrap()
            .ir
    );
}

#[test]
fn floats_and_wide_integers_keep_the_std_body() {
    let mut source = String::new();
    for name in ["f32", "f64", "i128", "i128u"] {
        source.push_str(&format!(
            "def sum_{name} :: ref [{name}] -> {name}\nfn sum_{name} values = Array.sum values\ndef best_{name} :: ref [{name}] -> bool\nfn best_{name} values = Maybe.is_some (Array.max values) && Maybe.is_some (Array.min values)\n"
        ));
    }
    source.push_str("export def probe :: i64\nfn probe =\n    let a = [1.0f32]\n    let b = [1.0]\n    let c = [1i128]\n    let d = [1i128u]\n    let sums = (sum_f32 (ref a) as i64) + (sum_f64 (ref b) as i64) + (sum_i128 (ref c) as i64) + (sum_i128u (ref d) as i64)\n    if best_f32 (ref a) && best_f64 (ref b) && best_i128 (ref c) && best_i128u (ref d) then sums else 0\n");
    let module = analyze(&source).unwrap_or_else(|error| panic!("{error:?}"));
    let sources = sources(&source);
    let ir = llvm::emit_native_build(&module, options(), &sources, None, false)
        .unwrap()
        .ir;
    assert!(!ir.contains("tsuzuri_cpu"));
}

#[test]
fn equivalent_call_forms_share_one_kernel_call() {
    let source = "def next :: i64 -> i64\nfn next state = state * 6364136223846793005 + 1442695040888963407\n\ndef data :: i64 -> [i32]\nfn data length = Array.init length (index -> (next (index + 1)) as i32)\n\ndef total :: Numeric<'a> => ref ['a] -> 'a\nfn total values = Array.sum values\n\nexport def forms :: i64\nfn forms =\n    let values = data 1000\n    let f = Array.sum\n    let a = Array.sum (ref values)\n    let b = total (ref values)\n    let c = f (ref values)\n    let d = Array.sum (ref values[3..997])\n    (a as i64) + (b as i64) + (c as i64) + (d as i64)\n";
    let module = analyze(source).unwrap_or_else(|error| panic!("{error:?}"));
    let sources = sources(source);
    let ir = llvm::emit_native_build(&module, options(), &sources, None, false)
        .unwrap()
        .ir;
    assert_eq!(ir.matches("call i32 @tsuzuri_cpu_sum_i32(").count(), 1);
}

#[test]
fn cpu_kernels_are_native_build_only() {
    let source = integer_kernels();
    let module = analyze(&source).unwrap();
    let sources = sources(&source);
    for ir in [
        llvm::emit(&module, llvm::Entry::Library).unwrap(),
        llvm::emit_target(&module, llvm::Entry::Library, true).unwrap(),
        llvm::emit_native_build(&module, options(), &sources[..1], None, false)
            .unwrap()
            .ir,
    ] {
        assert!(!ir.contains("tsuzuri_cpu"));
    }
}
