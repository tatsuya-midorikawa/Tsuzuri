# ABI: ABI

The rules of this area, one sentence each, with the document that states them and the conformance tests that check them (`tests/conformance/ABI/`). See `spec/README.md` for the format.

| ID | Rule | Doc | Tests | Status |
| --- | --- | --- | --- | --- |
| ABI-001 | An exported function MUST be declared in the header as `tz_<name>` with C types. | [language.md § 公開 ABI](../../docs/language.md#公開-abi) | ABI-001-exported-function-must-be | active |
| ABI-002 | A function that is not exported MUST NOT appear in the header. | [language.md § 公開 ABI](../../docs/language.md#公開-abi) | ABI-002-function-that-is-not | active |
| ABI-003 | A `unit` result MUST map to `void`. | [unit-type.md](../../_tsuzuri/language-reference/types-and-type-inference/unit-type.md) | ABI-003-unit-result-must-map | active |
| ABI-004 | An exported function MUST NOT take a record parameter (E1008). | [language.md § 公開 ABI](../../docs/language.md#公開-abi) | ABI-004-exported-function-must-not | active |
| ABI-005 | An exported function MUST NOT take an owned `string` parameter (E1008). | [language.md § 公開 ABI](../../docs/language.md#公開-abi) | ABI-005-exported-function-must-not | active |
| ABI-006 | A `bool` MUST map to `int32_t` in the header. | [language.md § 公開 ABI](../../docs/language.md#公開-abi) | ABI-006-bool-must-map-to | active |
| ABI-007 | An `f64` MUST map to `double` in the header. | [language.md § 公開 ABI](../../docs/language.md#公開-abi) | ABI-007-f64-must-map-to | active |
| ABI-008 | A narrow integer such as `i8` MUST travel as a normalized 32-bit value. | [language.md § 公開 ABI](../../docs/language.md#公開-abi) | ABI-008-narrow-integer-such-as | active |
| ABI-009 | The header MUST be a C header usable from C++: `#pragma once`, `<stdint.h>` and an `extern "C"` guard. | [language.md § 公開 ABI](../../docs/language.md#公開-abi) | ABI-009-header-must-be-c | active |
| ABI-010 | An `i64u` MUST map to `uint64_t` and a narrow unsigned integer such as `i16u` MUST map to `uint32_t` in the header. | [language.md § 公開 ABI](../../docs/language.md#公開-abi) | ABI-010-i64u-must-map-to | active |
