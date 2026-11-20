# Changelog

## Unreleased

- Fix a workspace folder that is not a project (no `Main.tz` or `Tsuzuri.toml` at its top) with a package below it. The folder's language server also analyzed the package's files without the package's `Tsuzuri.toml`: false errors stayed in them after every fix, diagnostics, hovers, symbols, and completions showed twice, the hover, definitions, and symbols of the other files of the folder came back empty, and a large folder failed with `E1017` on a directory. A file without a marker now belongs to the highest directory up to the workspace folder that holds no other project and that the compiler can read at once (at most 1024 directories and 4096 sources, no directory 16 levels down, and no source or directory symbolic links). A file directly beside a package still reads the folder with the package, as `tsuzuri check` does, but that language server now serves only the files directly in the folder and leaves the package's files to the package's language server.
- Creating or deleting a file or folder below a workspace folder (such as `Main.tz`, `Tsuzuri.toml`, or a package's folder; not inside a folder whose name starts with `.`, or in a knowledge graph under `target/tz/`) decides the projects again: the language servers whose project changed restart, and the Testing view drops the projects that are no longer projects. A deleted or moved project folder's language server stops, and a file left open in it gets none.
- The language server starts in a temporary folder instead of the project folder, so on Windows a project folder can be deleted or moved while its language server runs.
- **Compatibility:** in a folder that holds a package, a file without a marker now belongs to a smaller project, so its module name counts from that project, and a reference to a file in another directory of the folder is `E1002`, as with `tsuzuri check` on the file. A folder without a package that the compiler can read at once keeps one project, as before.

## 0.1.8 - 2026-11-20

- Fix the extension's `Tsuzuri` copies of errors and warnings, and the task markers, that stayed in the Problems panel and as squiggles after they were fixed. The language server alone reports the errors and warnings of the code as you type. The test discovery that runs after each save no longer adds a `Tsuzuri` copy of a saved error; a failed discovery shows on the project in the Testing view.
- The results of a test run, a test debug, and Debug Project (source `Tsuzuri`) stay until the project changes: an edit, a created or written source, `Tsuzuri.toml`, or `Tsuzuri.lock`, a deleted file or folder (also a folder moved or renamed away), or a removed workspace folder. A folder moved into the project from elsewhere and a created or rebuilt `[native]` link input do not count; run the tests again after them. A result in a path dependency outside the project also clears when a Tsuzuri source outside the project changes or a folder there is deleted, and a run whose project changed while it ran shows no result. The results leave out what the language server already reports at the same place with the same code, keep what it does not check (such as `E1036`, link errors (`E2002`), and failed tests (`E2006`)), and place a diagnostic on the first line of a file with a UTF-8 byte order mark at the right column.
- On Windows, the results of a test run, a test debug, and Debug Project now appear on the file they name instead of a separate `\\?\` path, and the Testing view links tests to the files as VS Code opened them; a project opened through a symbolic link is handled the same way.
- **Compatibility:** the Run, Build, Check, Test, Debug, and Wasm tasks of the extension no longer report to the Problems panel, whose task markers stayed until the next task. Their diagnostics print to the terminal, where `path:line:column` opens the location. The `$tsuzuri` problem matcher remains for your own `tasks.json`.

## 0.1.7 - 2026-11-19

- Refresh the bundled compiler, standard library, completions, and offline handbook with the merged language and tooling updates.
- Add tzroutines, lightweight threads on an M:N scheduler for native macOS, Linux, and Windows: `tzroutine { }` and the `Routine` module (such as `go`, `yield`, `scope`, and `with_locked_thread`), Go-style two-way `Channel` with `SendOnly` / `ReceiveOnly` ends, the `select` expression, timers (`Time.sleep`, `after`, `tick`, `after_func`, `Timer`, `Ticker`), the sync types `WaitGroup`, `Once`, `OnceValue`, `RwMutex`, `Cond`, `Semaphore`, and `SyncMap`, and `Context` / `ErrGroup` for cancellation. `main`, exports, tests, and bench samples wait for the tzroutines they spawn. Blocking calls and socket waits no longer stop the other tzroutines, and long-running tzroutines are preempted. On WebAssembly, tzroutines take turns on the single JavaScript thread with `--wasm-feature jspi`. Native programs also run `Parallel`, `Mutex`, and `DataChannel` on this scheduler.
- **Compatibility:** the channel with separate sender and receiver ends is now `DataChannel` (`DataChannel.bounded`, `Sender`, `Receiver`, `clone_sender`). The old `Channel` names are `E1040`, and `tsuzuri fix` rewrites them.
- **Compatibility:** `task { }`, `Task<T>`, and `Task.*` are removed (`E1040`). Use `tzroutine { }`, channels, or `Routine.scope`; `tsuzuri fix` rewrites `Task.scope` to `Routine.scope`. `task` is an ordinary identifier again.
- **Compatibility:** `Async`, `async { }`, and the `Net.*_async` functions are removed (`E1040`) without an automatic fix. Call the synchronous `Net` functions inside a tzroutine instead.
- **Compatibility:** `Owned.function` rejects a function value that holds a borrow (`E1013`), and in-place writes such as `Array.write` and `Vec.push_mut` keep the borrows they store. Earlier versions accepted both kinds of program, which then read freed memory.
- Add anonymous unions `(A | B)` with injection, widening, and narrowing, the type-test pattern `:? T [as x]`, and the built-in `Eq`, `Ord`, `Hash`, `Display`, and JSON `Encode` / `Decode` when every member has them.
- New Project reserves the `Routine`, `DataChannel`, `SendOnly`, `ReceiveOnly`, `Timer`, `Ticker`, `WaitGroup`, `Once`, `OnceValue`, `RwMutex`, `Cond`, `Semaphore`, `SyncMap`, `Context`, and `ErrGroup` namespaces consistently with the compiler, and no longer reserves `Async`, `Task`, or `task`. Syntax highlighting adds `tzroutine` and the `select` arms (`recv ... from`, `send`, `default`) and drops `task`, `Task`, and `Async`.
- Signature help finds the call on a line that follows a closing bracket.
- Fix a false `E1003` for call results passed to `ref` parameters and for lambdas with `ref` / `ref mut` parameter annotations, a false `E1013` for call results that contain functions, `E2002` for builtins over `dyn` types with markers such as `Channel<dyn (C, Send)>`, a data race with `--allocator small` in nested `Parallel` calls, and `tsuzuri bindgen` reading an unnamed `typedef struct`, `union`, or `enum` as an unrelated tag with Clang 23.

## 0.1.6 - 2026-11-01

- Refresh the bundled compiler, standard library, completions, and offline handbook with the merged language and tooling updates.
- **Compatibility:** edition 2027, the default for new projects, now accepts only the canonical spellings: `..=` / `..<` ranges, type-name literal suffixes such as `86i8u`, `ref` / `ref mut` / `deref` / `ref T`, and `\x -> e` lambdas. The other spellings are `E0004`; edition 2026 still accepts them and reports `W1007` with `--warn canonical`. `tsuzuri fix --edition 2027` rewrites them.
- **Compatibility:** `tsuzuri test` and the Testing view now fail a test whose integer arithmetic wraps. Write intended wrapping with `Int.wrapping_add`, `wrapping_sub`, `wrapping_mul`, or `wrapping_neg`, or pass `--allow-overflow` to report it as a warning instead.
- Diagnostics carry related locations, `help` lines, fixes, and a stable kind. The Problems panel and hovers show the related locations, the error hover names the kind for `tsuzuri explain`, quick fixes offer the fixes that the language server verified, and **Fix all auto-fixable problems** (`source.fixAll.tsuzuri`) applies a document's `certain` fixes. The CLI adds `tsuzuri explain` and `tsuzuri fix`.
- New Project also writes `AGENTS.md` and `llms.txt` for AI agents, and its `.gitignore` excludes `target/`, where the knowledge graph is built. The bundled compiler adds `tsuzuri guide`, the knowledge-graph commands (`tsuzuri kg`, `tsuzuri find`, `tsuzuri test --affected`), and the read-only MCP server `tsuzuri mcp`.
- Include the language and library additions: typed holes `?` with suggested candidates, `@requires` / `@ensures` contracts (`--contracts off|check|test|prove`), the checked effect attributes `@no_alloc`, `@no_copy`, `@no_trap`, and `@terminates`, `@trusted` externs with `[capabilities]` policies and `tsuzuri audit`, `--overflow-report`, any integer type for counted loops, indexes, and slice bounds, the `Text` class for `string` and `utf8string`, `Monoid` with a sequential-equivalent `Parallel.fold`, and trap backtraces with `Domain.run`.
- Hovers show inferred effects, contract lines, and the rendered examples of `///` comments; `tsuzuri test --doc` runs those examples.
- New Project reserves the `Text`, `RegexText`, `Domain`, `Monoid`, and `Monoids` namespaces consistently with the compiler.
- Fix lending casts, unary operators, `@checked`, and `if` / `match` / `try` / block values to `ref` parameters, which reported `E1003`.

## 0.1.5 - 2026-10-25

- Refresh the bundled compiler, standard library, completions, and offline handbook with the merged language and tooling updates.
- **Compatibility:** new projects and inputs without a manifest now use edition 2027, with shared-borrowed read parameters and explicit `own T` / `mut T` conventions. Existing manifests without an edition remain on 2026. Use `tsuzuri fix --edition 2027 --from 2026` to migrate while preserving ownership and callback contracts.
- Add checked `pure def` contracts, including generic functions and staged callbacks. `tsuzuri api-diff --effective` compares resolved ownership, purity, and region contracts; the default comparison remains syntax-only.
- Include the chunked ordered `Map` and `Set` implementations and matching debugger collection views, while preserving key order and ownership rules.
- Add the `Alloc` module and isolated per-iteration allocation counts through `tsuzuri bench --allocs`. New Project now reserves the `Alloc` namespace consistently with the compiler.
- Fix interpolation of borrowed places and temporary values, including the corresponding ownership and lifetime handling.

## 0.1.4 - 2026-10-23

- Update the bundled compiler, standard library, completions, and offline handbook with the merged `Array`, `List`, and `Vec` API improvements.
- **Breaking change:** standard-library transformations take their subject last so pipelines and function composition work consistently. Update existing calls to forms such as `Array.get index array`, `Map.insert key value map`, and `String.sub first count text`; resource-handle, numeric, compiler-protocol, and test/benchmark entry points retain their documented ordering.
- Expand the F#-style `Array` and `List` functions and their borrowing variants, and provide .NET `List<T>`-style `Vec` operations with subject-last arguments.
- Include the collection sorting and hashing fixes for NaN values and borrowed comparers/keys, with the behavior and limitations documented in the bundled module reference pages.

## 0.1.3 - 2026-10-22

- Require an Apple Silicon (ARM64) Clang when building and verifying the bundled macOS toolchain, and reject Intel macOS launcher hosts and targets even when a target is specified explicitly.
- Reject Intel macOS Clang targets in `tsuzuri bindgen` before generating bindings, with an explicit supported-platform diagnostic.
- Align macOS setup instructions and the bundled handbook with Apple Silicon-only support. Windows x64/ARM64, Linux x64/ARM64, and macOS ARM64 remain the five package targets; Windows ARM64 source debugging remains unsupported.

## 0.1.2 - 2026-10-21

- Support only Windows x64/ARM64, Linux x64/ARM64, and macOS ARM64. macOS x64 (Intel) packages are no longer released; Windows ARM64 continues to support editing, building, running, and testing without source debugging.
- Reject unsupported extension hosts before starting the language server or resolving a development compiler, and apply the same host checks to toolchain verification, VSIX creation and verification, and IDE integration tests.
- Check the five supported platforms consistently across CI, distribution settings, and the development and publishing documentation.
- New Project asks for the project namespace, checks it with the rules of `tsuzuri new`, and creates the project with the bundled `tsuzuri new`, so `Tsuzuri.toml` always sets `namespace` and `Main.tz` declares it.
- Highlighting, semantic tokens, and keyword completion for `namespace` and `using` declarations; module completion and go to definition follow namespaces and `using`.
- Namespace paths use `::` (`namespace Sample::Features`, `Sample::Features::Shape.area`): highlighting colors each namespace segment, completion offers a namespace's modules and namespaces after `::`, details show full names such as `module Demo::Shapes::Circle`, and New Project accepts namespaces such as `Acme::Tools`.
- The standard library is the namespace `std`: completion offers its modules after `std::` and the members of `std::Maybe.`, and New Project rejects namespaces that start with `std`. The standard `Option` is now `Maybe`.
- Snippets for `namespace`, `using`, `try ... with`, and `try ... with ... finally`. The `main` snippet writes `def main :: unit -> i32`, which returns the exit code, and the new `mainargs` snippet writes `def main :: Array<string> -> i32`, which receives the command-line arguments; these are the only entry-point signatures.
- Grammar tests cover literal suffixes, `@checked` / `@literal`, bit operators, and the error-handling keywords.
- **Tsuzuri: Open Documentation** opens the new Japanese language reference (`_tsuzuri/language-reference/index.md`), which replaces the previous `_docs` handbook in the offline bundle.
- Debugging shows Tsuzuri values: the debug launch loads the bundled LLDB formatters, so variables show strings, arrays, lists, unions (`Some(40)`, `Error("bad")`), `Map`, `Set`, and function values as Tsuzuri values; the call stack shows Tsuzuri function names such as `Main.show`, and stepping skips the runtime and generated helpers.
- The Testing view has a **Debug** profile: it builds the selected test alone with debug information, starts it in CodeLLDB so breakpoints in the test body stop, and reports the result from the runner's exit code.
- Highlighting, keyword completion, and a snippet for `bench` declarations; `Bench` and `Gen` are reserved standard library modules, so New Project rejects them as namespaces.
- A `#!` shebang on the first line of a file (`#!/usr/bin/env tsuzuri script`) is highlighted as a comment.

## 0.1.0

- Bundled compiler, LLVM, Zig SDK, and an offline CodeLLDB installer.
- Highlighting, diagnostics, hover, definitions, outline, formatting, snippets, and basic completions.
- Build, run, WASM output, Test Explorer, and source debugging without project configuration files.
- x64 and ARM64 packaging. Windows ARM64 supports all workflows except source debugging; x86 is excluded.
- Workspace Trust, isolated process execution, Unicode positions, multi-root projects, and offline documentation.
- Real VS Code integration tests, relocated-toolchain tests, and platform-specific VSIX integrity checks.
