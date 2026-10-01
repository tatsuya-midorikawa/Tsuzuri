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

## CI and toolchain distribution

Lessons from the PR #3 and #4 VS Code workflow failures:

- Treat a red CI job as blocking. A failed step skips every later step on that
  runner, so PR #3's failing Windows archive smoke hid the Windows integration,
  VSIX, and installed-extension steps. Compare with earlier runs (`gh run list`)
  to separate inherited failures from new ones.
- Fetch pinned third-party archives with `download()` in
  `scripts/toolchain/bundle.mjs`: it tries each source in order, verifies the
  pinned SHA-256, and skips a source silent for 30 seconds. Do not depend on one
  origin with a total timeout; ziglang.org is a single server, so Zig comes from
  shuffled community mirrors first. Refresh that snapshot when bumping Zig.
- `fs::canonicalize` returns verbatim `\\?\` paths on Windows. Use
  `cache::real_path` for paths shown to users or passed to child tools, and
  normalize expected paths in tests the same way.
- Type-check Windows code from other hosts with
  `cargo check --all-targets --target x86_64-pc-windows-msvc` (and
  `aarch64-pc-windows-msvc`) using a rustup toolchain with those targets.
  Windows behavior is verified only by a green Windows CI job.
- `gh run view <run> --log-failed` shows failed steps. Logs of finished jobs in a
  running workflow come from `gh api repos/<owner>/<repo>/actions/jobs/<job>/logs`.
