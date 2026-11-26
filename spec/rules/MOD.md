# MOD: Modules

The rules of this area, one sentence each, with the document that states them and the conformance tests that check them (`tests/conformance/MOD/`). See `spec/README.md` for the format.

| ID | Rule | Doc | Tests | Status |
| --- | --- | --- | --- | --- |
| MOD-001 | A module file name MUST start with an uppercase letter (E1011). | [language.md § 1 ファイル = 1 モジュール](../../docs/language.md#1-ファイル--1-モジュール) | MOD-001-module-file-name-must | active |
| MOD-002 | A public member MUST be reachable as `Module.member` from another module. | [language.md § 1 ファイル = 1 モジュール](../../docs/language.md#1-ファイル--1-モジュール) | MOD-002-public-member-must-be | active |
| MOD-003 | A `private` member MUST NOT be reachable from another module (E1022). | [language.md § 1 ファイル = 1 モジュール](../../docs/language.md#1-ファイル--1-モジュール) | MOD-003-private-member-must-not | active |
| MOD-004 | A public function MUST NOT expose a private type of its module (E1022). | [language.md § 1 ファイル = 1 モジュール](../../docs/language.md#1-ファイル--1-モジュール) | MOD-004-public-function-must-not | active |
| MOD-005 | A subdirectory MUST extend the module name with `::` when no namespace is declared. | [language.md § ソースと宣言](../../docs/language.md#ソースと宣言) | MOD-005-subdirectory-must-extend-module | active |
| MOD-006 | `using` MUST allow a module of the named namespace to be referenced without the namespace. | [language.md § ソースと宣言](../../docs/language.md#ソースと宣言) | MOD-006-using-must-allow-module | active |
| MOD-007 | `using` of an unknown namespace MUST be rejected (E1011). | [language.md § ソースと宣言](../../docs/language.md#ソースと宣言) | MOD-007-using-of-unknown-namespace | active |
| MOD-008 | A top-level expression MUST be accepted only in `Main.tz` (E2004 elsewhere). | [language.md § ソースと宣言](../../docs/language.md#ソースと宣言) | MOD-008-top-level-expression-must | active |
| MOD-009 | A `def` in a `.tt` file MUST be rejected unless it belongs to a class (E1018). | [language.md § 1 ファイル = 1 モジュール](../../docs/language.md#1-ファイル--1-モジュール) | MOD-009-def-in-tt-file | active |
| MOD-010 | A module name that is also a record name in the same file MUST be referenced by the module name alone. | [language.md § 1 ファイル = 1 モジュール](../../docs/language.md#1-ファイル--1-モジュール) | MOD-010-module-name-that-is | active |
