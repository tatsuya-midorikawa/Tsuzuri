# Development policy

Tsuzuri aims for maximum practical performance as well as safety and clarity.
This applies to the compiler, runtime, builtins, and every future standard-library API.
Read `docs/architecture.md` and `docs/language.md` before changing their contracts.

- Prefer typed LLVM operations and intrinsics over generic software dispatch when
  they preserve the language semantics. Equivalent builtin and operator forms
  must not accidentally select different performance paths.
- Design bulk operations for contiguous data, specialization, SIMD, and appropriate
  CPU parallelism or GPU backends. Choose by end-to-end cost, including allocation,
  dispatch, transfers, synchronization, and startup; using more hardware is not
  itself a performance result.
- Preserve overflow, rounding, NaN, signed zero, saturation, evaluation order,
  traps, ownership, and borrowing. Do not silently enable fast-math, reassociation,
  races, unsupported instructions, or double rounding to win a benchmark.
- Keep a correct portable path and make hardware requirements explicit. Automatic
  backend selection needs capability checks and a documented fallback; an
  explicitly requested unavailable backend must report an error.
- Add boundary and reference tests for optimized paths and fallbacks. Check native
  and WASM at `-O0` and `-O3` when numeric semantics or shared lowering changes.
  Keep intrinsic declarations deduplicated and generated IR deterministic.
- Measure matched workloads and inspect generated code before claiming SIMD,
  parallel, or GPU acceleration. Report actual support separately from planned
  support. Do not add speed pass/fail thresholds to shared CI runners.

Build and validation commands are in `README.md`. Reproducible performance
comparisons and their limitations are in `docs/benchmarks.md`.
