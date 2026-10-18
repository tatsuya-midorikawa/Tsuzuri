---
description: "Use when changing GitHub Actions workflows, the bundled toolchain (Zig, LLVM, clang, scripts/toolchain), the VS Code extension packaging, build caches or path handling, or when debugging a CI failure on Windows, macOS or Linux. Lessons from the Windows and VS Code workflow failures."
applyTo: [".github/**", "scripts/toolchain/**", "vsc/**", "tests/windows.*", "src/cache.rs"]
---
# CI and toolchain distribution

Lessons from the PR #3 and #4 VS Code workflow failures, and from the Windows gate (G10):

- Treat a red CI job as blocking. A failed step skips every later step on that runner, so PR #3's failing Windows
  archive smoke hid the Windows integration, VSIX, and installed-extension steps. Compare with earlier runs
  (`gh run list`) to separate inherited failures from new ones.
- Which workflow runs when: `ci.yml` (fast gate) on every pull request that touches code or tests, `reference.yml` on one that
  touches the language reference (and every night); `targets.yml`, `windows.yml` and `vscode.yml` on every push to
  `main` and, on a pull request, only with the label `ci:full`. Put the label on a change that can behave differently per
  OS or toolchain, and watch the first post-merge run of these three.
- Fetch pinned third-party archives with `download()` in `scripts/toolchain/bundle.mjs`: it tries each source in order,
  verifies the pinned SHA-256, skips a source silent for 30 seconds, and makes up to three passes because GitHub release
  downloads also return transient 5xx errors. Do not depend on one origin with a total timeout; ziglang.org is a single
  server, so Zig comes from shuffled community mirrors first. Refresh that snapshot when bumping Zig.
- `fs::canonicalize` returns verbatim `\\?\` paths on Windows. Use `cache::real_path` for paths shown to users or
  passed to child tools, and normalize expected paths in tests the same way.
- Type-check Windows code from other hosts with `cargo check --all-targets --target x86_64-pc-windows-msvc` (and
  `aarch64-pc-windows-msvc`) using a rustup toolchain with those targets. Windows behavior is verified only by a green
  Windows CI job.
- `gh run view <run> --log-failed` shows failed steps. Logs of finished jobs in a running workflow come from
  `gh api repos/<owner>/<repo>/actions/jobs/<job>/logs`.
- The bundled Windows x64 toolchain compiles C with `zig cc`, which defines `NDEBUG` from `-O1` up, unlike clang
  elsewhere. A C test host that calls the code under test inside `assert(...)` then skips the call, and later steps see
  a state that never happened (the Async host test died with `0xC000001D` at `-O1` and passed at `-O0`). Start such
  hosts with `#undef NDEBUG`, and read a pass of an optimized run as meaningful only if its checks were compiled in.
- `windows.yml` is the Windows gate (G10). Its `cargo test --locked --no-fail-fast` takes 40 to 50 minutes on
  `windows-2022` and comes last, so a failure of the other steps can be read at once. The first Windows run of a step is
  rarely the last: dispatch the workflow on your branch with `fast` set (`gh workflow run windows.yml --ref <branch>
  -f fast=true`), which skips that step and reads every other step in about 10 minutes, and merge the fixes back. The log
  of a running job cannot be read.
- To debug a Windows-only failure without 25-minute loops, push a throwaway branch whose workflow runs `on: push` for
  that branch only, cache `vsc/toolchain` with `actions/cache` (about 4 minutes per loop once cached), print what the
  failing program does (progress lines, an unhandled-exception filter that prints the code and address), and delete the
  branch, its runs, and its cache afterwards.
- A Rust test that runs clang or git must also pass with the MSVC toolchain: there is no `-lm` (LNK1181) and no
  compiler-rt (`__udivti3`; append `src/runtime/wasm.ll` as `windows_abi` does), `link.exe` writes its errors to stdout,
  a trap ends the process with `0xC000001D` instead of a signal, a C stdout is in text mode (CRLF), diagnostics spell
  paths with backslashes, and Git for Windows cannot read a configuration file that a verbatim path (`\\?\`) names.
