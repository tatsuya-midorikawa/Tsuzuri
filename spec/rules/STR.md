# STR: Strings

The rules of this area, one sentence each, with the document that states them and the conformance tests that check them (`tests/conformance/STR/`). See `spec/README.md` for the format.

| ID | Rule | Doc | Tests | Status |
| --- | --- | --- | --- | --- |
| STR-001 | The length of a `string` MUST count UTF-16 code units. | [strings.md](../../_tsuzuri/language-reference/literals-and-strings/strings.md) | STR-001-length-of-string-must | active |
| STR-002 | The length of a `utf8string` MUST count UTF-8 bytes. | [strings.md](../../_tsuzuri/language-reference/literals-and-strings/strings.md) | STR-002-length-of-utf8string-must | active |
| STR-003 | There MUST be no implicit conversion between `string` and `utf8string`. | [strings.md](../../_tsuzuri/language-reference/literals-and-strings/strings.md) | STR-003-there-must-be-no | active |
| STR-004 | `+` on two strings MUST concatenate them in order. | [language.md § string と utf8string](../../docs/language.md#string-と-utf8string) | STR-004-on-two-strings-must | active |
| STR-005 | String equality MUST compare contents, not identity. | [language.md § string と utf8string](../../docs/language.md#string-と-utf8string) | STR-005-string-equality-must-compare | active |
| STR-006 | Strings MUST compare lexicographically with `<`. | [language.md § string と utf8string](../../docs/language.md#string-と-utf8string) | STR-006-strings-must-compare-lexicographically | active |
| STR-007 | An interpolation hole MUST borrow its value, so the value stays usable afterwards. | [interpolated-strings.md](../../_tsuzuri/language-reference/literals-and-strings/interpolated-strings.md) | STR-007-interpolation-hole-must-borrow | active |
| STR-008 | `{{` and `}}` MUST denote literal braces in an interpolated string. | [interpolated-strings.md](../../_tsuzuri/language-reference/literals-and-strings/interpolated-strings.md) | STR-008-and-must-denote-literal | active |
| STR-009 | An out-of-range string index MUST trap. | [language.md § string と utf8string](../../docs/language.md#string-と-utf8string) | STR-009-out-of-range-string | active |
| STR-010 | An escape such as `\n` MUST denote one character. | [language.md § string と utf8string](../../docs/language.md#string-と-utf8string) | STR-010-escape-such-as-n | active |
