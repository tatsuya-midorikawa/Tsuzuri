# EDN: Editions

The rules of this area, one sentence each, with the document that states them and the conformance tests that check them (`tests/conformance/EDN/`). See `spec/README.md` for the format.

| ID | Rule | Doc | Tests | Status |
| --- | --- | --- | --- | --- |
| EDN-001 | A manifest without `edition` MUST be accepted and use edition 2026 (the manifest default), not the latest edition. | [stability.md § edition](../../docs/stability.md#edition) | EDN-001-manifest-without-edition-must | active |
| EDN-002 | `edition = "2026"` MUST be accepted. | [stability.md § edition](../../docs/stability.md#edition) | EDN-002-edition-2026-must-be | active |
| EDN-003 | An unsupported edition MUST be rejected with E0002 and the message MUST list the supported editions. | [stability.md § edition](../../docs/stability.md#edition) | EDN-003-unsupported-edition-must-be | active |
| EDN-004 | An unknown key in `[package]` MUST be rejected with E0002. | [stability.md § edition](../../docs/stability.md#edition) | EDN-004-unknown-key-in-package | active |
| EDN-005 | `tsuzuri new` MUST write the latest edition (`edition = "2027"`) into the manifest. | - | - | retired |
| EDN-006 | The reserved words of edition 2026 MUST NOT be usable as identifiers. | [language.md § ソースと宣言](../../docs/language.md#ソースと宣言) | EDN-006-reserved-words-of-edition | active |
| EDN-007 | Input without a manifest MUST use the latest edition. | [stability.md](../../docs/stability.md) | EDN-007-input-without-manifest-must | active |
| EDN-008 | The edition MUST be a string: an integer value MUST be rejected with E0002. | [stability.md § edition](../../docs/stability.md#edition) | EDN-008-edition-must-be-string | active |
| EDN-009 | A manifest without `[package]` MUST be rejected. | [stability.md § edition](../../docs/stability.md#edition) | EDN-009-manifest-without-package-must | active |
| EDN-010 | An unsupported edition MUST be rejected with E0002 before the sources are read, so no source diagnostic is reported in the same run. | [stability.md § edition](../../docs/stability.md#edition) | EDN-010-unsupported-edition-must-be | active |
| EDN-011 | `edition = "2028"` MUST be accepted. | [stability.md § edition](../../docs/stability.md#edition) | EDN-011-edition-2028-must-be | active |
| EDN-012 | `tsuzuri new` MUST write the latest edition (`edition = "2028"`) into the manifest. | [README.md § CLI リファレンス](../../README.md#cli-リファレンス) | EDN-012-tsuzuri-new-must-write | active |
