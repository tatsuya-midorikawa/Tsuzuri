use std::{collections::BTreeSet, path::Path};

use tsuzuri::driver::{BuildOptions, Emit, Project, Target};
use tsuzuri::llvm::{self, Allocator, EmitOptions, Entry};
use tsuzuri::trap::{TrapKind, TrapSource};

const HOST_HEAP: &str = include_str!("../src/runtime/heap-host.ll");
const NATIVE_HEAP: &str = include_str!("../src/runtime/heap-native.ll");
const COUNTING_HEAP: &str = include_str!("../src/runtime/heap-counting.ll");
const SMALL_HEAP: &str = include_str!("../src/runtime/heap-small.ll");
const SMALL_SHIM: &str = include_str!("../src/runtime/heap-small-native.ll");

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
        runtime_objects: false,
        wasm_simd128: false,
        allocator,
        f16: llvm::F16Path::Soft,
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
fn small_allocator_swaps_only_the_heap_runtime() {
    let module = fixture();
    let system = emit(&module, false, Allocator::System);
    let small = emit(&module, false, Allocator::Small);
    assert_eq!(small, emit(&module, false, Allocator::Small));
    // The heap is the end of the module, so what precedes it is the same with either allocator.
    assert!(system.ends_with(NATIVE_HEAP));
    let program = &system[..system.len() - NATIVE_HEAP.len()];
    assert!(small.starts_with(program));
    assert!(small.ends_with(SMALL_SHIM));
    let heap = &small[program.len()..];
    assert!(heap.starts_with("; Generated from heap-small.c by generate_heap.py"));
    // The generated runtime keeps its lines; only a declaration that the program has already made is dropped once.
    for line in SMALL_HEAP
        .lines()
        .filter(|line| !line.starts_with("declare"))
    {
        assert!(heap.contains(line), "{line}");
    }
    let count = |text: &str, needle: &str| text.matches(needle).count();
    // LLVM drops an allocation that is freed unread only while it sees a call to a function with the allocation attributes,
    // so the two entry points stay calls (noinline) and carry them; realloc is opaque.
    assert_eq!(
        count(&small, "define internal noalias ptr @tz_small_alloc("),
        1
    );
    assert_eq!(
        count(
            &small,
            "define internal void @tz_small_free(ptr noundef allocptr "
        ),
        1
    );
    assert_eq!(
        count(
            &small,
            "noinline allockind(\"alloc,uninitialized\") allocsize(0) \"alloc-family\"=\"tz_small\" {"
        ),
        1
    );
    assert_eq!(
        count(
            &small,
            "noinline allockind(\"free\") \"alloc-family\"=\"tz_small\" {"
        ),
        1
    );
    assert_eq!(count(&small, "define internal ptr @tz_small_realloc("), 1);
    assert_eq!(count(&small, "define weak hidden void @tz_small_stats("), 1);
    assert_eq!(count(&small, "@tz_small_shared = weak hidden global"), 1);
    assert_eq!(count(&small, " thread_local "), 1);
    assert_eq!(
        small
            .lines()
            .filter(|line| line.starts_with("define") && line.contains("@tz.alloc("))
            .count(),
        1
    );
    // The C library serves the blocks over 512 bytes; nothing else of the system is needed.
    for declaration in [
        "declare ptr @malloc(",
        "declare ptr @realloc(",
        "declare void @free(",
    ] {
        assert_eq!(count(&small, declaration), 1, "{declaration}");
    }
    for call in [
        "@aligned_alloc",
        "@posix_memalign",
        "@mmap",
        "@calloc",
        "@pthread_",
    ] {
        assert!(!small.contains(call), "{call}");
    }
    // The default output never mentions the size classes.
    assert!(!system.contains("tz_small"));
    assert_eq!(emit(&module, false, Allocator::System), system);
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
        if !wasm {
            assert_eq!(kinds(Allocator::Small), system);
        }
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
    // The size classes add nothing to the interface.
    assert_eq!(
        llvm::header_with_allocator(&module, Allocator::Small),
        plain
    );
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
        let executable = BuildOptions {
            emit: Emit::Executable,
            allocator,
            ..base
        };
        if allocator == Allocator::Counting {
            executable.validate().unwrap();
        } else {
            reject(
                executable,
                "requires object, LLVM IR, header, or WebAssembly output",
            );
        }
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
fn small_allocator_is_for_native_outputs() {
    let reject = |options: BuildOptions, message: &str| {
        let error = options.validate().unwrap_err();
        assert_eq!(error.code, "E2000");
        assert!(error.message.contains(message), "{}", error.message);
    };
    let base = BuildOptions {
        emit: Emit::Object,
        allocator: Allocator::Small,
        ..BuildOptions::default()
    };
    for emit in [Emit::Executable, Emit::Object, Emit::Llvm, Emit::Header] {
        BuildOptions { emit, ..base }.validate().unwrap();
    }
    for target in [Target::Wasm32, Target::Wasm64] {
        reject(
            BuildOptions {
                target,
                emit: Emit::Wasm,
                ..base
            },
            "--allocator small requires a native target",
        );
    }
    reject(
        BuildOptions {
            emit: Emit::Wgsl,
            ..base
        },
        "--allocator small requires executable, object, shared library, LLVM IR, or header output",
    );
    reject(
        BuildOptions {
            trap_return: true,
            ..base
        },
        "--trap-mode return keeps its own tracked allocator",
    );
    reject(
        BuildOptions {
            freestanding: true,
            ..base
        },
        "--freestanding requires --allocator host",
    );
    // The library API refuses a WebAssembly module as well.
    let error = llvm::emit_with_options(&fixture(), options(true, Allocator::Small)).unwrap_err();
    assert_eq!(error.code, "E2000");
    assert!(
        error
            .message
            .contains("the small allocator is for native targets")
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
