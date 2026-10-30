# CLI: CLI

The rules of this area, one sentence each, with the document that states them and the conformance tests that check them (`tests/conformance/CLI/`). See `spec/README.md` for the format.

| ID | Rule | Doc | Tests | Status |
| --- | --- | --- | --- | --- |
| CLI-001 | An unknown option MUST be rejected with E2000 and exit code 2. | [option.md](../../_tsuzuri/language-reference/compiler/option.md) | CLI-001-unknown-option-must-be | active |
| CLI-002 | `run` MUST print the value of the final expression of `Main.tz` and exit with 0. | [language.md § アプリケーションのエントリーポイント](../../docs/language.md#アプリケーションのエントリーポイント) | CLI-002-run-must-print-value | active |
| CLI-003 | A program that exits with a non-zero code MUST make `run` fail with E2005 and exit code 1. | [usage.md](../../_tsuzuri/language-reference/compiler/usage.md) | CLI-003-program-that-exits-with | active |
| CLI-004 | `fmt --check` MUST exit with 1 and name the file when a source is not formatted. | [usage.md](../../_tsuzuri/language-reference/compiler/usage.md) | CLI-004-fmt-check-must-exit | active |
| CLI-005 | `fmt --check` MUST exit with 0 and print nothing when every source is formatted. | [usage.md](../../_tsuzuri/language-reference/compiler/usage.md) | CLI-005-fmt-check-must-exit | active |
| CLI-006 | `test --list` MUST print `index module name` lines for each test and MUST NOT run them. | [test.md](../../_tsuzuri/language-reference/built-in-types-and-modules/test.md) | CLI-006-test-list-must-print | active |
| CLI-007 | `test --filter` with no match MUST exit with 0 and report every test as ignored. | [test.md](../../_tsuzuri/language-reference/built-in-types-and-modules/test.md) | CLI-007-test-filter-with-no | active |
| CLI-008 | `new` MUST create `Tsuzuri.toml`, `Main.tz` and `.gitignore`. | [usage.md](../../_tsuzuri/language-reference/compiler/usage.md) | CLI-008-new-must-create-tsuzuri | active |
| CLI-009 | `build --emit llvm` MUST produce byte-identical IR for the same input. | [architecture.md § 不変条件](../../docs/architecture.md#不変条件) | CLI-009-build-emit-llvm-must | active |
| CLI-010 | `api-diff` MUST exit with 1 when a public declaration was removed. | [usage.md](../../_tsuzuri/language-reference/compiler/usage.md) | CLI-010-api-diff-must-exit | active |
