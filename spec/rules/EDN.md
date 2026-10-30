# EDN: Editions

The rules of this area, one sentence each, with the document that states them and the conformance tests that check them (`tests/conformance/EDN/`). See `spec/README.md` for the format.

| ID | Rule | Doc | Tests | Status |
| --- | --- | --- | --- | --- |
| EDN-001 | A manifest without `edition` MUST be accepted and use edition 2026 (the manifest default), not the latest edition. | [packages.md](../../_tsuzuri/language-reference/organizing-tsuzuri/packages.md) | EDN-001-manifest-without-edition-must | active |
| EDN-002 | `edition = "2026"` MUST be accepted. | [packages.md](../../_tsuzuri/language-reference/organizing-tsuzuri/packages.md) | EDN-002-edition-2026-must-be | active |
| EDN-003 | An unsupported edition MUST be rejected with E0002 and the message MUST list the supported editions. | [packages.md § 言語版（edition）](../../_tsuzuri/language-reference/organizing-tsuzuri/packages.md#言語版edition) | EDN-003-unsupported-edition-must-be | active |
| EDN-004 | An unknown key in `[package]` MUST be rejected with E0002. | [packages.md](../../_tsuzuri/language-reference/organizing-tsuzuri/packages.md) | EDN-004-unknown-key-in-package | active |
| EDN-005 | `tsuzuri new` MUST write the latest edition (`edition = "2027"`) into the manifest. | [usage.md](../../_tsuzuri/language-reference/compiler/usage.md) | EDN-005-tsuzuri-new-must-write | active |
| EDN-006 | The reserved words of edition 2026 MUST NOT be usable as identifiers. | [keywords.md](../../_tsuzuri/language-reference/values-and-functions/keywords.md) | EDN-006-reserved-words-of-edition | active |
| EDN-007 | Input without a manifest MUST use the latest edition. | [stability.md](../../docs/stability.md) | EDN-007-input-without-manifest-must | active |
| EDN-008 | The edition MUST be a string: an integer value MUST be rejected with E0002. | [packages.md](../../_tsuzuri/language-reference/organizing-tsuzuri/packages.md) | EDN-008-edition-must-be-string | active |
| EDN-009 | A manifest without `[package]` MUST be rejected. | [packages.md](../../_tsuzuri/language-reference/organizing-tsuzuri/packages.md) | EDN-009-manifest-without-package-must | active |
| EDN-010 | An unsupported edition MUST be rejected with E0002 before the sources are read, so no source diagnostic is reported in the same run. | [packages.md § 言語版（edition）](../../_tsuzuri/language-reference/organizing-tsuzuri/packages.md#言語版edition) | EDN-010-unsupported-edition-must-be | active |
