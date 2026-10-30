# Development policy

Tsuzuri aims for maximum practical performance as well as safety and clarity.
This applies to the compiler, runtime, builtins, and every future standard-library API.
`docs/architecture.md` and `docs/language.md` define the contracts: read the sections you change (see Working agreement).

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

Build and validation commands are in `README.md` (検証と性能測定). Reproducible performance
comparisons and their limitations are in `docs/benchmarks.md`.

## Working agreement

- Read what the change needs. `docs/architecture.md` (500 KB) and `docs/language.md` (700 KB) are references: list the
  headings with `grep -n '^#' <file>`, then read that range. A ticket in `_features/` or `_perfs/` starts from
  `node scripts/repo-kb.mjs brief <ID>` (including GUIDE-LITE and the ticket sections), and uses
  `_features/GUIDE.md` only for the sections the ticket cites.
  `_features/_completed/` and `_perfs/_completed/` are history that `.ignore` keeps out of searches; name the directory
  to search it (`rg E1005 _features/_completed`).
- One ticket or fix per branch and per pull request. Commit on your branch as you go; never commit to or push `main`.
  Push the branch when CI should see it, and no more often than that: a push cancels the running jobs of the PR.
- Sessions in several worktrees share one machine. Wrap heavy commands in `node scripts/slot.mjs --` (for example
  `node scripts/slot.mjs -- cargo nextest run --locked`): it waits for one of the shared slots. Run a linux/amd64
  container only to check a change that needs one; CI has native x86-64 Linux.
- Keep scratch output under `target/scratch/<name>/` and delete it when the work is done.

## Standard library API conventions

These rules cover every public function of `std/` and every compiler builtin that a program can call. They keep
pipelines (`x |> M.f a`) and compositions (`M.f a >> M.g b`) working in every module, as in F#.
`tests/std_argument_order.rs` enforces them on the `def` lines of every `std/` file and on the schemes of the `Array`,
`List`, `Vec`, `String` and `Utf8String` builtins: it fails for a function that breaks the rule and for a `std/` file that
its `MODULES` table does not classify. Record of the decision: `_features/GUIDE.md` D-59. User-facing text:
`_tsuzuri/language-reference/built-in-types-and-modules/argument-order.md`.

- **The subject is the last argument.** The value that a function reads, transforms or updates (a collection, a
  string, a `Maybe` or `Result`, an `IO` action, a matrix or tensor view, an arena, a writer) is its last parameter.
  Everything else (keys, indices, counts, positions, callbacks, separators) comes before it, in the order that F# or
  .NET gives those arguments: `Map.insert key value map`, `String.sub first count text`, `Maybe.bind f maybe`.
  When several arguments are subjects (`append`, `zip`, `union`, `mul`), keep the F# order; when a function writes
  into one of them, the written one is last (`MatrixView.copy_from source target`).
- Constructors and functions without a subject (`empty`, `init`, `of_array`, `parse`) are not affected. A borrowing
  twin (`get_ref`, `foldr`) takes the same order as the function it mirrors.
- The subject is lent last, so an argument before it may read it (`Array.write 0 (v[1] * 10) v`); the pipeline form
  lends it first (`v |> Array.write 0 (v[1] * 10)` is `E1014`).
- When a function also takes a value of the subject's own type (a needle and a text: `String.find`,
  `Regex.replace_all`), the types cannot tell them apart. In `String`, `Utf8String` and `Regex`, name the last
  parameter `text` (`parts` for a list of texts); the test checks the name.
- **Exemptions are these four kinds and nothing else** (a `std/` file's exemption carries its reason in its `MODULES`
  row; a module that is only a compiler builtin has no file, so this list is its record):
  resource handles (`File`, `Net`, `Process`, `Atomic`, `Mutex`, `Channel`, `Rc`, `Arc`, `Owned`, `Task`: the handle
  comes first, like the receiver of a .NET method); numeric functions (`Int`, `Math`, `Simd`, `Char`: operand order of
  the formula); compiler protocol (the capitalised builder operations `Bind`, `Return`..., and the helpers that
  `src/derive.rs` calls); test and benchmark entry points (`Gen.for_all`, `Test.*`, `Bench.*`). A pure
  transformation is never exempt.
- A function that the compiler recognises by its name or position (`String.slice` in `src/llvm_strings.rs`, the
  builtins `Utf8String.decode_at`, `Array.__set`, `Array.update`, `Array.swap`, `Array.write`, `Array.swap_in` and
  `Array.split_at_mut`, which `Builtin::name` in `src/check.rs` lists) changes together with its hook: search `src/`
  for the name before you reorder, and run the compact-strings suites.
- New module or function: decide the subject first and write it last; add the file to `MODULES` (`subject(&["Type"])`,
  or an exemption with its reason); give the module page an "引数の順序" section and keep `argument-order.md` true.

## Verification

Pick the cheapest tier that can show the change is right. Repeat T0 and T1 while you work and run T2 once before you
report the work done; T3 is CI. Do not run the whole suite before you start: the latest `ci.yml` run on `main`
(`gh run list --branch main --workflow ci.yml --limit 1`) is the baseline, and a failure that `main` does not have is yours.

| Tier | Run | Takes |
| --- | --- | --- |
| T0, every edit | `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, the tests of the area (README, 変更の種類別) | seconds |
| T1, every step | `cargo nextest run --locked` (`cargo install --locked cargo-nextest`; without it `cargo test --locked`) | 2 to 6 min |
| T2, before done | T1, the node suites of the area on the dev binary (`cargo build`, then `node tests/<suite>.mjs target/debug/tsuzuri`), `node scripts/check-docs.mjs <changed pages>`, and `sh scripts/check-runtime-includes.sh` after a change in `src/runtime/` | 10 to 20 min |
| T3, CI | the release build, every suite, every platform, fuzzers, sanitizers, benchmarks | CI |

The dev profile is `opt-level = 1` (`Cargo.toml`), so the Rust tests and the dev binary are fast enough: build
`--release` for a performance claim or a suite that needs it, not before every suite. `cargo test <name>` succeeds when
it runs 0 tests: check `running N tests`.

## CI

- A pull request gets `ci.yml` (about 20 minutes: fmt, clippy, a type check of the Windows targets, the Rust tests, the core
  end-to-end suites) and, when it changes the language reference, `reference.yml` (the changed pages). `targets.yml`,
  `windows.yml` and `vscode.yml` run after every merge to `main` and on a pull request with the label `ci:full`; add it when
  the change can behave differently per OS or toolchain (`src/runtime/`, `scripts/toolchain/`, caches, paths, child
  processes, line endings, `vsc/`).
- Read CI yourself: `gh run list --branch <branch>`, `gh run watch <id> --exit-status`, `gh run view <id> --log-failed`.
  Do not ask a person to paste logs. A red job is blocking. Read `.github/instructions/ci.instructions.md` (the Windows,
  toolchain-bundle and VS Code lessons; it also applies when you edit workflows, `scripts/toolchain/`, `vsc/`,
  `tests/windows.*` or `src/cache.rs`) before you debug a CI failure.

## Language reference

`_tsuzuri/language-reference/` (contents: `index.md`) is the user manual. Every
change that adds, changes, fixes, or removes user-visible behavior updates the
related pages in the same pull request: once, when the behavior has settled, not
after every step. This covers syntax, semantics, types,
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
  and `-O3`, and runs examples that contain tests. `reference.yml` runs it for the
  pages a pull request changes, and every night for all pages.
- A block between `<!-- generated:NAME -->` and `<!-- /generated:NAME -->` (the diagnostic kinds, the
  complexity tables of the std collections, the benchmark tables) is made by `node scripts/gen-docs.mjs`:
  do not edit it by hand. After changing its source (`std/` tags, the diagnostic registry,
  `docs/benchmarks-data/*.json`), run `node scripts/gen-docs.mjs`; `node scripts/gen-docs.mjs --check` and
  `node scripts/check-cli-docs.mjs` (flags, commands, variables, codes, versions and std functions that the
  documents miss) run in `docs.yml`.
- Keep `docs/language.md` consistent in the same pull request. `README.md`
  states the quickstart, the feature list, the CLI synopsis and the validation
  commands: change it when the change makes one of those wrong. Pages
  are Japanese; see `_features/GUIDE.md` §8. If no page needs an update, say so
  in the PR description.

## For AI agents working on tickets

Run `node scripts/repo-kb.mjs brief <TICKET-ID>` before opening a whole ticket or GUIDE.md.
Use `--phase N` for another phase or `--max-chars N` for a longer pack. The pack includes the
ticket sections, cited decisions, public symbols, test commands, and GUIDE-LITE. Each
`[shortened` or `[omitted` line names the command for retrieving the missing text, for example
`node scripts/repo-kb.mjs section G22 spec`.

Other lookups: `ticket <ID>`, `decision D-45`, `files <path>`, `diag E1012`, `search <words>`.
The index under `target/repo-kb/` is refreshed when inputs change. Report contradictions rather
than improvising; `node scripts/check-guide-lite.mjs` detects stale references.
