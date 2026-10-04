# Changelog

## Unreleased

- New Project asks for the project namespace, checks it with the rules of `tsuzuri new`, and creates the project with the bundled `tsuzuri new`, so `Tsuzuri.toml` always sets `namespace` and `Main.tz` declares it.
- Highlighting, semantic tokens, and keyword completion for `namespace` and `using` declarations; module completion and go to definition follow namespaces and `using`.
- Snippets for `namespace`, `using`, `try ... with`, and `try ... with ... finally`; the `main` snippet writes with `IO.writeln`.
- Grammar tests cover literal suffixes, `@checked` / `@literal`, bit operators, and the error-handling keywords.

## 0.1.0

- Bundled compiler, LLVM, Zig SDK, and an offline CodeLLDB installer.
- Highlighting, diagnostics, hover, definitions, outline, formatting, snippets, and basic completions.
- Build, run, WASM output, Test Explorer, and source debugging without project configuration files.
- x64 and ARM64 packaging. Windows ARM64 supports all workflows except source debugging; x86 is excluded.
- Workspace Trust, isolated process execution, Unicode positions, multi-root projects, and offline documentation.
- Real VS Code integration tests, relocated-toolchain tests, and platform-specific VSIX integrity checks.
