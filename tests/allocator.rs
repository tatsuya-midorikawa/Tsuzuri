use std::{collections::BTreeSet, path::Path};

use tsuzuri::driver::{BuildOptions, Emit, Project, Target};
use tsuzuri::llvm::{self, Allocator, EmitOptions, Entry};
use tsuzuri::trap::{TrapKind, TrapSource};

const HOST_HEAP: &str = include_str!("../src/runtime/heap-host.ll");
const NATIVE_HEAP: &str = include_str!("../src/runtime/heap-native.ll");
const COUNTING_HEAP: &str = include_str!("../src/runtime/heap-counting.ll");

fn fixture() -> tsuzuri::check::CheckedModule {
    Project::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/host_abi/Main.tz"))
        .unwrap()
        .analyze()
        .unwrap()
}

fn options(wasm: bool, allocator: Allocator) -> EmitOptions {
    EmitOptions {
        entry: Entry::Library,
        wasm,
        debug_output: false,
        allocator,
    }
}

fn emit(module: &tsuzuri::check::CheckedModule, wasm: bool, allocator: Allocator) -> String {
    llvm::emit_with_options(module, options(wasm, allocator)).unwrap()
}

#[test]
fn system_allocator_output_is_unchanged() {
    let module = fixture();
    for wasm in [false, true] {
        assert_eq!(
            emit(&module, wasm, Allocator::System),
            llvm::emit_target(&module, Entry::Library, wasm).unwrap()
        );
    }
}

#[test]
fn host_allocator_swaps_only_the_heap_runtime() {
    let module = fixture();
    let system = emit(&module, false, Allocator::System);
    let host = emit(&module, false, Allocator::Host);
    assert_eq!(host, emit(&module, false, Allocator::Host));
    assert!(system.contains(NATIVE_HEAP) && host.contains(HOST_HEAP));
    assert_eq!(
        host.replacen(HOST_HEAP, "", 1),
        system.replacen(NATIVE_HEAP, "", 1)
    );
    assert_eq!(
        host.matches("declare ptr @tsuzuri_host_alloc(i64, i64)")
            .count(),
        1
    );
    for libc in ["@malloc", "@free(", "@realloc("] {
        assert!(!host.contains(libc), "{libc}");
    }
    // A WASM module imports the three functions instead of linking them.
    let wasm = emit(&module, true, Allocator::Host);
    for name in ["alloc", "free", "realloc"] {
        assert!(
            wasm.contains(&format!(
                "\"wasm-import-module\"=\"tsuzuri_heap\" \"wasm-import-name\"=\"{name}\""
            )),
            "{name}"
        );
    }
    assert!(
        !wasm.contains("@tz.heap.free = "),
        "the module keeps no free list"
    );
}

#[test]
fn counting_allocator_wraps_the_target_heap() {
    let module = fixture();
    for wasm in [false, true] {
        let counting = emit(&module, wasm, Allocator::Counting);
        assert_eq!(counting, emit(&module, wasm, Allocator::Counting));
        assert!(counting.contains(COUNTING_HEAP));
        assert!(counting.contains("define void @tsuzuri_alloc_stats(ptr %stats)"));
        for function in ["@tz.alloc(", "@tz.free(", "@tz.realloc("] {
            let base = function.replace('(', ".base(");
            assert_eq!(
                counting
                    .lines()
                    .filter(|line| line.starts_with("define") && line.contains(function))
                    .count(),
                1,
                "{function}"
            );
            assert_eq!(
                counting
                    .lines()
                    .filter(|line| line.starts_with("define") && line.contains(&base))
                    .count(),
                1,
                "{base}"
            );
        }
        if !wasm {
            assert!(counting.contains("declare ptr @malloc(i64)"));
        }
    }
}

#[test]
fn allocators_keep_allocation_failure_traps() {
    let source = "export def make :: i64 -> [i64]\nfn make count = new [i64](count, index -> index)\nexport def total :: i64 -> i64\nfn total count = Array.sum (make count)";
    let module = tsuzuri::analyze(source).unwrap();
    let sources = [TrapSource {
        path: "Main.tz",
        text: source,
    }];
    for wasm in [false, true] {
        let kinds = |allocator| {
            llvm::emit_with_trap_info(&module, options(wasm, allocator), &sources)
                .unwrap()
                .trap_sites
                .iter()
                .map(|site| site.kind)
                .collect::<BTreeSet<_>>()
        };
        let system = kinds(Allocator::System);
        assert!(system.contains(&TrapKind::AllocationFailure), "{system:?}");
        assert_eq!(kinds(Allocator::Host), system, "wasm: {wasm}");
        assert_eq!(kinds(Allocator::Counting), system, "wasm: {wasm}");
    }
}

#[test]
fn headers_list_the_allocator_functions() {
    let module = fixture();
    let plain = llvm::header(&module);
    assert_eq!(
        plain,
        llvm::header_with_allocator(&module, Allocator::System)
    );
    assert!(!plain.contains("tsuzuri_host_alloc") && !plain.contains("tsuzuri_alloc_stats"));
    let host = llvm::header_with_allocator(&module, Allocator::Host);
    let closing = host.rfind("#ifdef __cplusplus").unwrap();
    for prototype in [
        "void *tsuzuri_host_alloc(uint64_t size, uint64_t align);",
        "void tsuzuri_host_free(void *ptr, uint64_t size, uint64_t align);",
        "void *tsuzuri_host_realloc(void *ptr, uint64_t old_size, uint64_t new_size, uint64_t align);",
    ] {
        let at = host
            .find(prototype)
            .unwrap_or_else(|| panic!("{prototype}"));
        assert!(at < closing, "{prototype} is inside extern \"C\"");
    }
    let counting = llvm::header_with_allocator(&module, Allocator::Counting);
    let closing = counting.rfind("#ifdef __cplusplus").unwrap();
    let at = counting
        .find("void tsuzuri_alloc_stats(tsuzuri_allocation_stats *stats);")
        .unwrap();
    assert!(at < closing);
    assert!(counting.contains("#ifndef TSUZURI_ALLOCATION_STATS_DEFINED"));
}

#[test]
fn allocator_options_reject_unsupported_outputs() {
    let reject = |options: BuildOptions, message: &str| {
        let error = options.validate().unwrap_err();
        assert_eq!(error.code, "E2000");
        assert!(error.message.contains(message), "{}", error.message);
    };
    let base = BuildOptions {
        emit: Emit::Object,
        ..BuildOptions::default()
    };
    for allocator in [Allocator::Host, Allocator::Counting] {
        for emit in [Emit::Object, Emit::Llvm, Emit::Header] {
            BuildOptions {
                emit,
                allocator,
                ..base
            }
            .validate()
            .unwrap();
        }
        for target in [Target::Wasm32, Target::Wasm64] {
            BuildOptions {
                target,
                emit: Emit::Wasm,
                allocator,
                ..base
            }
            .validate()
            .unwrap();
        }
        reject(
            BuildOptions {
                emit: Emit::Executable,
                allocator,
                ..base
            },
            "requires object, LLVM IR, header, or WebAssembly output",
        );
        reject(
            BuildOptions {
                allocator,
                trap_return: true,
                ..base
            },
            "--trap-mode return keeps its own tracked allocator",
        );
    }
    BuildOptions {
        target: Target::Wasm32,
        emit: Emit::Wasm,
        wasm_threads: true,
        allocator: Allocator::Counting,
        ..base
    }
    .validate()
    .unwrap();
    reject(
        BuildOptions {
            target: Target::Wasm32,
            emit: Emit::Wasm,
            wasm_threads: true,
            allocator: Allocator::Host,
            ..base
        },
        "--allocator host cannot be combined with --wasm-feature threads",
    );
    let error = llvm::emit_with_options(&fixture(), options(true, Allocator::Host));
    assert!(
        error.is_ok(),
        "the library API accepts a WASM host allocator"
    );
}

#[test]
fn freestanding_needs_a_host_allocator_and_native_library_output() {
    let reject = |options: BuildOptions, message: &str| {
        let error = options.validate().unwrap_err();
        assert_eq!(error.code, "E2000");
        assert!(error.message.contains(message), "{}", error.message);
    };
    let freestanding = BuildOptions {
        emit: Emit::Object,
        allocator: Allocator::Host,
        freestanding: true,
        ..BuildOptions::default()
    };
    for emit in [Emit::Object, Emit::Llvm, Emit::Header] {
        BuildOptions {
            emit,
            ..freestanding
        }
        .validate()
        .unwrap();
    }
    reject(
        BuildOptions {
            target: Target::Wasm32,
            emit: Emit::Wasm,
            ..freestanding
        },
        "--freestanding requires a native target",
    );
    reject(
        BuildOptions {
            emit: Emit::Executable,
            ..freestanding
        },
        "requires object, LLVM IR, header, or WebAssembly output",
    );
    reject(
        BuildOptions {
            allocator: Allocator::System,
            ..freestanding
        },
        "--freestanding requires --allocator host",
    );
    reject(
        BuildOptions {
            trap_info: true,
            ..freestanding
        },
        "--freestanding cannot be combined with --trap-info or --debug-output",
    );
    reject(
        BuildOptions {
            debug_output: true,
            ..freestanding
        },
        "--freestanding cannot be combined with --trap-info or --debug-output",
    );
}
