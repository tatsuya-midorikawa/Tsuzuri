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

## Language reference

`_tsuzuri/language-reference/` (contents: `index.md`) is the user manual. Every
change that adds, changes, fixes, or removes user-visible behavior updates the
related pages in the same commit or PR. This covers syntax, semantics, types,
builtins, `std` APIs, diagnostic codes and messages, CLI options, targets, host
interop, documented performance characteristics, and known limitations or bugs.

- Find every affected page by searching for the feature's names, keywords,
  diagnostic codes, and CLI flags, for example
  `grep -rnE "Int\.min|E1005|--emit" _tsuzuri/language-reference`.
- Update the prose, examples and their `run=` output, tables, diagrams, and
  `index.md`. Document a new feature where readers will look for it. For a
  removed feature, delete its text and examples and name the replacement.
- When a planned feature ships or a known bug is fixed, replace the planned
  note or known-bug warning, and its `_features/` or `_perfs/` link, with the
  actual behavior.
- Describe only what the current compiler does. Check claims against the
  implementation and keep planned support separate.
- Run `node scripts/check-docs.mjs <changed pages>`. It checks links and
  anchors, type-checks every example, runs `run=` examples natively at `-O0`
  and `-O3`, and runs examples that contain tests.
- Keep `docs/language.md` and `README.md` consistent in the same change. Pages
  are Japanese; see `_features/GUIDE.md` §8. If no page needs an update, say so
  in the PR description.

## CI and toolchain distribution

Lessons from the PR #3 and #4 VS Code workflow failures:

- Treat a red CI job as blocking. A failed step skips every later step on that
  runner, so PR #3's failing Windows archive smoke hid the Windows integration,
  VSIX, and installed-extension steps. Compare with earlier runs (`gh run list`)
  to separate inherited failures from new ones.
- Fetch pinned third-party archives with `download()` in
  `scripts/toolchain/bundle.mjs`: it tries each source in order, verifies the
  pinned SHA-256, skips a source silent for 30 seconds, and makes up to three
  passes because GitHub release downloads also return transient 5xx errors.
  Do not depend on one origin with a total timeout; ziglang.org is a single
  server, so Zig comes from shuffled community mirrors first. Refresh that
  snapshot when bumping Zig.
- `fs::canonicalize` returns verbatim `\\?\` paths on Windows. Use
  `cache::real_path` for paths shown to users or passed to child tools, and
  normalize expected paths in tests the same way.
- Type-check Windows code from other hosts with
  `cargo check --all-targets --target x86_64-pc-windows-msvc` (and
  `aarch64-pc-windows-msvc`) using a rustup toolchain with those targets.
  Windows behavior is verified only by a green Windows CI job.
- `gh run view <run> --log-failed` shows failed steps. Logs of finished jobs in a
  running workflow come from `gh api repos/<owner>/<repo>/actions/jobs/<job>/logs`.
- The bundled Windows x64 toolchain compiles C with `zig cc`, which defines
  `NDEBUG` from `-O1` up, unlike clang elsewhere. A C test host that calls the
  code under test inside `assert(...)` then skips the call, and later steps see
  a state that never happened (the Async host test died with `0xC000001D` at
  `-O1` and passed at `-O0`). Start such hosts with `#undef NDEBUG`, and read a
  pass of an optimized run as meaningful only if its checks were compiled in.
- To debug a Windows-only failure without 25-minute loops, push a throwaway
  branch whose workflow runs `on: push` for that branch only, cache
  `vsc/toolchain` with `actions/cache` (about 4 minutes per loop once cached),
  print what the failing program does (progress lines, an unhandled-exception
  filter that prints the code and address), and delete the branch, its runs, and
  its cache afterwards.
