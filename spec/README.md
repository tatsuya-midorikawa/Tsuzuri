# Language rules (G33)

`spec/rules/<AREA>.md` holds the rules of the language and its tools that the conformance suite checks, one English sentence each. A rule has a stable ID `AREA-NNN` (D-58): `LEX` lexical, `SYN` syntax, `TYP` types, `OWN` ownership, `EVL` evaluation order, `NUM` numerics, `STR` strings, `CTL` control, `MOD` modules, `TSK` tasks, `DIA` diagnostics, `CLI` commands, `ABI` exported functions, `STD` standard library, `EDN` editions.

Each table has the columns `| ID | Rule | Doc | Tests | Status |`:

- **ID**: numbered from 001 in each area, in order. A number is never reused; a rule whose meaning changes becomes `retired` and the new meaning gets the next number.
- **Rule**: one sentence that says `MUST`, `MUST NOT` or `MAY` (never `SHOULD`), at most 240 characters, ending with a period. A rule of one edition starts with `In edition 2026, ` or `In edition 2027, `.
- **Doc**: the one link to the document that states the rule (`docs/*.md`, `_tsuzuri/language-reference/**`, `README.md`). `node scripts/check-spec.mjs --link PAGE "HEADING"` prints it.
- **Tests**: the stems of the tests under `tests/conformance/<AREA>/` that name the rule. `check-spec --write` fills it; do not edit it by hand.
- **Status**: `active` or `retired`.

A test is `tests/conformance/<AREA>/<ID>-<words>.tz`, or a directory of that name with a `Main.tz` and more files. The last lines of its `Main.tz` say what it checks:

```
let first = "data"
let second = first
first.length

// rule: OWN-001
// expect: check-error E1012 line=3
```

The expectations are `check-ok`, `check-error CODE [line=N] [file=NAME] [kind=K]`, `check-warning CODE …`, `stdout "TEXT"`, `trap kind=K [line=N] [stdout "TEXT"]` and `tests-pass N`; `// cli: ARGS` runs the compiler with `ARGS` instead (`exit`, `stdout`, `stdout-has`, `stderr-has`, `stderr-lacks`, `stdout-empty`, `stderr-empty`, `stderr-lines`, `file-has`, `file-lacks`, `deterministic`). `// edition: 2026` checks a single-file test in that edition; `// targets:` narrows the compiler runs.

To add a rule:

1. Check the sentence of the document on the compiler (`check --json`, `run -O0` and `-O3`, `test --target wasm32` for numbers). Add nothing that the compiler does not do.
2. Append a row with the next number of the area, `Tests` `-` and `Status` `active`.
3. Write a test that ends with `// rule:` and `// expect:` lines.
4. `node scripts/conformance.mjs target/release/tsuzuri --rule ID` passes.
5. `node scripts/check-spec.mjs --write`, then `node scripts/check-spec.mjs` passes.
6. Put `<!-- rule: ID -->` before the statement in the Doc file; `node scripts/check-spec.mjs --require-markers` and `node scripts/check-docs.mjs PAGE` pass.

The language reference page `compiler/conformance.md` shows the coverage of each area.
