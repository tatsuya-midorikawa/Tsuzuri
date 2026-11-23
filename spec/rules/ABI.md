# ABI: ABI

The rules of this area, one sentence each, with the document that states them and the conformance tests that check them (`tests/conformance/ABI/`). See `spec/README.md` for the format.

| ID | Rule | Doc | Tests | Status |
| --- | --- | --- | --- | --- |
| ABI-001 | An exported function MUST be declared in the header as `tz_<name>` with C types. | [language.md § 公開 ABI](../../docs/language.md#公開-abi) | ABI-001-exported-function-must-be | active |
| ABI-002 | A function that is not exported MUST NOT appear in the header. | [language.md § 公開 ABI](../../docs/language.md#公開-abi) | ABI-002-function-that-is-not | active |
| ABI-003 | A `unit` result MUST map to `void`. | [unit-type.md](../../_tsuzuri/language-reference/types-and-type-inference/unit-type.md) | ABI-003-unit-result-must-map | active |
| ABI-004 | An exported function MUST NOT take a record parameter with a field that the C ABI cannot carry, such as `string` (E1008); records of scalar fields are allowed. | [language.md § 公開 ABI](../../docs/language.md#公開-abi) | ABI-004-exported-function-must-not | active |
| ABI-005 | An exported function MUST NOT take an owned `string` parameter (E1008). | [language.md § 公開 ABI](../../docs/language.md#公開-abi) | ABI-005-exported-function-must-not | active |
| ABI-006 | A `bool` MUST map to `int32_t` in the header. | [language.md § 公開 ABI](../../docs/language.md#公開-abi) | ABI-006-bool-must-map-to | active |
| ABI-007 | An `f64` MUST map to `double` in the header. | [language.md § 公開 ABI](../../docs/language.md#公開-abi) | ABI-007-f64-must-map-to | active |
| ABI-008 | A narrow integer such as `i8` MUST cross the C ABI as `int32_t`: truncated to its width on the way in and extended to 32 bits on the way out. | [language.md § 公開 ABI](../../docs/language.md#公開-abi) | ABI-008-narrow-integer-such-as, ABI-008-narrow-integer-wrapper | active |
| ABI-009 | The header MUST be a C header usable from C++: `#pragma once`, `<stdint.h>` and an `extern "C"` guard. | [native-interop.md § ヘッダーとオブジェクト](../../_tsuzuri/language-reference/compiler/native-interop.md#ヘッダーとオブジェクト) | ABI-009-header-must-be-c | active |
| ABI-010 | An `i64u` MUST map to `uint64_t` and a narrow unsigned integer such as `i16u` MUST map to `uint32_t` in the header. | [language.md § 公開 ABI](../../docs/language.md#公開-abi) | ABI-010-i64u-must-map-to | active |
| ABI-011 | A value of an `extern type copy` MUST be copied, and MUST cross the C ABI as the same pointer typedef as other handles. | [language.md § 不透明ハンドル](../../docs/language.md#不透明ハンドル) | ABI-011-copy-extern-type-header-and-copy | active |
| ABI-012 | Declarations of one extern symbol MUST agree on the import module, the number of C parameters, and the C ABI class of every parameter and of the result; they MAY differ in Tsuzuri types (E1008 otherwise). | [language.md § リンク名](../../docs/language.md#リンク名) | ABI-012-extern-symbol-c-abi-mismatch | active |
| ABI-013 | The IR MUST declare an extern symbol once even when several C-ABI-compatible declarations call it. | [language.md § リンク名](../../docs/language.md#リンク名) | ABI-013-extern-symbol-declared-once | active |
