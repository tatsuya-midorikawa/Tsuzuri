# Changelog

## Unreleased

- New Project asks for the project namespace, checks it with the rules of `tsuzuri new`, and creates the project with the bundled `tsuzuri new`, so `Tsuzuri.toml` always sets `namespace` and `Main.tz` declares it.
- Highlighting, semantic tokens, and keyword completion for `namespace` and `using` declarations; module completion and go to definition follow namespaces and `using`.
- Namespace paths use `::` (`namespace Sample::Features`, `Sample::Features::Shape.area`): highlighting colors each namespace segment, completion offers a namespace's modules and namespaces after `::`, details show full names such as `module Demo::Shapes::Circle`, and New Project accepts namespaces such as `Acme::Tools`.
- The standard library is the namespace `std`: completion offers its modules after `std::` and the members of `std::Maybe.`, and New Project rejects namespaces that start with `std`. The standard `Option` is now `Maybe`.
- Snippets for `namespace`, `using`, `try ... with`, and `try ... with ... finally`. The `main` snippet writes `def main :: unit -> i32`, which returns the exit code, and the new `mainargs` snippet writes `def main :: Array<string> -> i32`, which receives the command-line arguments; these are the only entry-point signatures.
- Grammar tests cover literal suffixes, `@checked` / `@literal`, bit operators, and the error-handling keywords.
- **Tsuzuri: Open Documentation** opens the new Japanese language reference (`_tsuzuri/language-reference/index.md`), which replaces the previous `_docs` handbook in the offline bundle.
- Debugging shows Tsuzuri values: the debug launch loads the bundled LLDB formatters, so variables show strings, arrays, lists, unions (`Some(40)`, `Error("bad")`), `Map`, `Set`, and function values as Tsuzuri values; the call stack shows Tsuzuri function names such as `Main.show`, and stepping skips the runtime and generated helpers.
- The Testing view has a **Debug** profile: it builds the selected test alone with debug information, starts it in CodeLLDB so breakpoints in the test body stop, and reports the result from the runner's exit code.

## 0.1.0

- Bundled compiler, LLVM, Zig SDK, and an offline CodeLLDB installer.
- Highlighting, diagnostics, hover, definitions, outline, formatting, snippets, and basic completions.
- Build, run, WASM output, Test Explorer, and source debugging without project configuration files.
- x64 and ARM64 packaging. Windows ARM64 supports all workflows except source debugging; x86 is excluded.
- Workspace Trust, isolated process execution, Unicode positions, multi-root projects, and offline documentation.
- Real VS Code integration tests, relocated-toolchain tests, and platform-specific VSIX integrity checks.
