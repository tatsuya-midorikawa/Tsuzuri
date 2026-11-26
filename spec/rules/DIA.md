# DIA: Diagnostics

The rules of this area, one sentence each, with the document that states them and the conformance tests that check them (`tests/conformance/DIA/`). See `spec/README.md` for the format.

| ID | Rule | Doc | Tests | Status |
| --- | --- | --- | --- | --- |
| DIA-001 | `check --json` MUST write one JSON object per diagnostic on stderr, with the keys `severity`, `code`, `message`, `path` and `span` first and in that order. | [language.md § 診断](../../docs/language.md#診断) | DIA-001-check-json-must-write | active |
| DIA-002 | A span MUST carry byte offsets `start` and `end` and 1-based `line`, `column`, `end_line` and `end_column`. | [language.md § 診断](../../docs/language.md#診断) | DIA-002-span-must-carry-byte | active |
| DIA-003 | A warning MUST NOT make `check` fail: the exit code MUST be 0. | [language.md § 診断](../../docs/language.md#診断) | DIA-003-warning-must-not-make | active |
| DIA-004 | An unused local MUST be reported as warning W1001 whose message names the fix of prefixing the name with `_`. | [language.md § 診断](../../docs/language.md#診断) | DIA-004-unused-local-must-be | active |
| DIA-005 | A lexical error MUST stop the compiler before parsing, so no syntax error is reported in the same run. | [language.md § 診断](../../docs/language.md#診断) | DIA-005-lexical-error-must-stop | active |
| DIA-006 | A syntax error MUST stop the compiler before type checking, so no type error is reported in the same run. | [language.md § 診断](../../docs/language.md#診断) | DIA-006-syntax-error-must-stop | active |
| DIA-007 | The human format MUST be `path:line:column: severity[code]: message` followed by the source line and a caret line. | [language.md § 診断](../../docs/language.md#診断) | DIA-007-human-format-must-be | active |
| DIA-008 | A `check` that finds no diagnostic MUST print nothing. | [README.md § CLI リファレンス](../../README.md#cli-リファレンス) | DIA-008-successful-check-must-print | active |
| DIA-009 | A diagnostic about a name MUST point at that name, not at the start of the statement. | [language.md § 診断](../../docs/language.md#診断) | DIA-009-diagnostic-about-name-must | active |
| DIA-010 | A warning in `--json` output MUST use the severity `warning`. | [language.md § 診断](../../docs/language.md#診断) | DIA-010-warning-in-json-output | active |
| DIA-011 | In edition 2027, a bare `..` slice with an end bound in a root-package source MUST produce warning W1013 (`legacy_bare_slice_range`) by default. | [language.md § 診断](../../docs/language.md#診断) | DIA-011-legacy-bare-slice-range | active |
| DIA-012 | A non-zero negative integer literal in a slice bound or built-in collection index MUST produce warning W1014 (`negative_literal_range_bound` or `negative_literal_index`). | [language.md § 診断](../../docs/language.md#診断) | DIA-012-negative-literal-index-and-bound | active |
