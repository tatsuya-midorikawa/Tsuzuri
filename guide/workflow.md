# Tsuzuri guide: workflow

## Project and commands
- `tsuzuri new app --namespace Acme::App` writes `Tsuzuri.toml` (`edition = "2027"`), `Main.tz`, `.gitignore`, `AGENTS.md`, `llms.txt`. Pass the project directory: `Main.tz` is the entry and every source below it is loaded.
- `check` type-checks; `run` builds and runs (`-O3`); `build -o out` keeps the executable; `test`; `bench`; `fmt` (`--check` in CI); `doc -o dir`; `fetch` gets git and registry dependencies into `Tsuzuri.lock`. `script FILE args` runs one file; `repl`; `watch check .` reruns on save.
- Exit codes: 0 success (warnings too, unless `--deny-warnings`), 1 diagnostics or failure, 2 bad arguments.

## Diagnostics
- `tsuzuri check . --json` writes one JSON object per line on stderr: `severity`, `code`, `message`, `path`, `span` (`line`, `column`, `end_line`, `end_column`, byte `start`/`end`), `kind`, then if present `related` (`role`: `moved_here`, `borrowed_here`...), `help`, `expected`/`found`, `scope` (visible locals), `fixes`.
- Codes: `E0xxx` lexing and syntax, `E1xxx` names, types, ownership and borrows, `E2xxx` tools, I/O, linking and runtime. Warnings: `W1xxx`, `W2xxx`.
- A syntax error stops type checking, and not every error shows in one pass. Fix the first diagnostic, then check again: later ones are often consequences.
- `tsuzuri explain KIND` shows why the rule exists, with a bad and a good example; `tsuzuri explain CODE` lists its kinds and fixes.
- `tsuzuri fix .` applies `certain` fixes and rechecks before writing (`--dry-run` shows the diff, `--applicability likely` applies more).

## Tests
- `test "name" = body` (a `unit` body) sits beside the code and is not in the executable. Use `assert (c)`, `Test.is_true c`, `Test.equal (ref a) (ref b)`. `Gen.for_all gen prop` tries 100 values with a fixed seed (`--seed N`).
- `tsuzuri test .` runs each test in its own process at `-O0`. `--list`, `--filter TEXT` (a substring of `Module.name`), `--json` (results on stdout). A failure exits 1 (E2006). A test whose integer `+ - *` wraps fails: write intended wrapping as `Int.wrapping_add`.
```tsuzuri
def double :: i64 -> i64 = \n -> n * 2

test "double" = Test.equal (ref (double 21)) (ref 42)
test "even" = Gen.for_all (Gen.range (-1000) 1000) (\n -> double n % 2 == 0)
```

## Performance and debugging
- `bench "name" = Bench.of (\_ -> work)`; `tsuzuri bench .` measures natively at `-O3` (median of 11 samples).
- A trap (index out of bounds, division by zero, `assert`) under `run` is E2005 with the source position (`build` needs `--trap-info` for that).
- `tsuzuri check . --warn implicit-copy` shows hidden O(n) copies (W1006).

## When stuck
- `tsuzuri guide --list`, `tsuzuri guide --topic ownership`, `tsuzuri guide std Array`, `tsuzuri explain E1014`.
- Reduce the failing code to a small `Main.tz` and check it alone.
<!--if edition=2026-->
- Edition 2026: `tsuzuri fix --edition 2027 --from 2026 .` migrates the project (`--dry-run` first).
<!--endif-->
