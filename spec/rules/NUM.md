# NUM: Numerics

The rules of this area, one sentence each, with the document that states them and the conformance tests that check them (`tests/conformance/NUM/`). See `spec/README.md` for the format.

| ID | Rule | Doc | Tests | Status |
| --- | --- | --- | --- | --- |
| NUM-001 | An integer literal without a suffix and without any typing context MUST have type `i32`. | [type-inference.md § 文脈が決まらない場合の既定型](../../_tsuzuri/language-reference/types-and-type-inference/type-inference.md#文脈が決まらない場合の既定型) | NUM-001-integer-literal-without-suffix | active |
| NUM-002 | Integer `+` MUST wrap modulo 2^width. | [language.md § 数値仕様](../../docs/language.md#数値仕様) | NUM-002-integer-must-wrap-modulo | active |
| NUM-003 | Integer `-` MUST wrap modulo 2^width. | [language.md § 数値仕様](../../docs/language.md#数値仕様) | NUM-003-integer-must-wrap-modulo | active |
| NUM-004 | Integer `*` MUST wrap modulo 2^width. | [language.md § 数値仕様](../../docs/language.md#数値仕様) | NUM-004-integer-must-wrap-modulo | active |
| NUM-005 | Unary minus of the minimum signed value MUST wrap to the same value. | [language.md § 数値仕様](../../docs/language.md#数値仕様) | NUM-005-unary-minus-of-minimum | active |
| NUM-006 | Integer `/` MUST truncate toward zero. | [language.md § 数値仕様](../../docs/language.md#数値仕様) | NUM-006-integer-must-truncate-toward | active |
| NUM-007 | Integer `%` MUST take the sign of the dividend. | [language.md § 数値仕様](../../docs/language.md#数値仕様) | NUM-007-integer-must-take-sign | active |
| NUM-008 | Integer division by zero MUST trap. | [language.md § 数値仕様](../../docs/language.md#数値仕様) | NUM-008-integer-division-by-zero | active |
| NUM-009 | Signed integer division of the minimum value by -1 MUST trap. | [language.md § 数値仕様](../../docs/language.md#数値仕様) | NUM-009-signed-integer-division-of | active |
| NUM-010 | Signed integer remainder of the minimum value by -1 MUST trap. | [language.md § 数値仕様](../../docs/language.md#数値仕様) | NUM-010-signed-integer-remainder-of | active |
| NUM-011 | A shift count MUST be masked with `bit_width - 1`. | [language.md § 数値仕様](../../docs/language.md#数値仕様) | NUM-011-shift-count-must-be | active |
| NUM-012 | `>>>` MUST be an arithmetic shift on signed integers and a logical shift on unsigned integers. | [language.md § 数値仕様](../../docs/language.md#数値仕様) | NUM-012-must-be-arithmetic-shift | active |
| NUM-013 | `**` MUST be right-associative. | [op-and-expressions.md](../../_tsuzuri/language-reference/values-and-functions/op-and-expressions.md) | NUM-013-must-be-right-associative | active |
| NUM-014 | Unary minus MUST bind tighter than `**`, so `-2 ** 2` is `(-2) ** 2`. | [op-and-expressions.md](../../_tsuzuri/language-reference/values-and-functions/op-and-expressions.md) | NUM-014-unary-minus-must-bind | active |
| NUM-015 | An integer `**` with a negative exponent MUST trap. | [language.md § 数値仕様](../../docs/language.md#数値仕様) | NUM-015-integer-with-negative-exponent | active |
| NUM-016 | The bitwise operators `&&&`, `\|\|\|`, `^^^` and `~~~` MUST work on the bits of the operand type. | [op-and-expressions.md](../../_tsuzuri/language-reference/values-and-functions/op-and-expressions.md) | NUM-016-bitwise-operators-and-must | active |
| NUM-017 | There MUST be no implicit conversion between integer types of different widths. | [cast.md](../../_tsuzuri/language-reference/types-and-type-inference/cast.md) | NUM-017-there-must-be-no | active |
| NUM-018 | Narrowing integer `as` MUST keep the low bits. | [cast.md](../../_tsuzuri/language-reference/types-and-type-inference/cast.md) | NUM-018-narrowing-integer-as-must | active |
| NUM-019 | Widening integer `as` MUST sign-extend signed values and zero-extend unsigned values. | [cast.md](../../_tsuzuri/language-reference/types-and-type-inference/cast.md) | NUM-019-widening-integer-as-must | active |
| NUM-020 | Float to integer `as` MUST truncate toward zero. | [cast.md](../../_tsuzuri/language-reference/types-and-type-inference/cast.md) | NUM-020-float-to-integer-as | active |
| NUM-021 | Float to integer `as` MUST saturate at the minimum and maximum of the target type. | [cast.md](../../_tsuzuri/language-reference/types-and-type-inference/cast.md) | NUM-021-float-to-integer-as | active |
| NUM-022 | A NaN converted to an integer by `as` MUST become 0. | [cast.md](../../_tsuzuri/language-reference/types-and-type-inference/cast.md) | NUM-022-nan-converted-to-integer | active |
| NUM-023 | Float division by zero MUST NOT trap; it MUST yield an infinity or NaN. | [basic-types.md](../../_tsuzuri/language-reference/types-and-type-inference/basic-types.md) | NUM-023-float-division-by-zero | active |
| NUM-024 | Every ordering and equality comparison with a NaN operand MUST be false, except `!=`, which MUST be true. | [language.md § 数値仕様](../../docs/language.md#数値仕様) | NUM-024-every-ordering-and-equality | active |
| NUM-025 | Negative zero MUST compare equal to zero and MUST keep its sign through division. | [language.md § 数値仕様](../../docs/language.md#数値仕様) | NUM-025-negative-zero-must-compare | active |
| NUM-026 | `f64` arithmetic MUST be IEEE 754 round-to-nearest-even without fused or reassociated operations. | [language.md § 数値仕様](../../docs/language.md#数値仕様) | NUM-026-f64-arithmetic-must-be | active |
| NUM-027 | Integer to float `as` MUST round to nearest, ties to even. | [cast.md](../../_tsuzuri/language-reference/types-and-type-inference/cast.md) | NUM-027-integer-to-float-as | active |
| NUM-028 | `f32` arithmetic MUST round to `f32` after each operation, without an `f64` intermediate. | [language.md § 数値仕様](../../docs/language.md#数値仕様) | NUM-028-f32-arithmetic-must-round | active |
| NUM-029 | An integer `+` inside `@checked` MUST raise `OverflowException` on overflow instead of wrapping. | [exception-handling.md](../../_tsuzuri/language-reference/exception-handling/exception-handling.md) | NUM-029-integer-inside-checked-must | active |
| NUM-030 | In edition 2026, a short suffix MUST fix the literal type: `l` is `i64`, `y` is `i8`, and `u` is `i32u`. | [language.md § 数値仕様](../../docs/language.md#数値仕様) | NUM-030-suffix-must-fix-literal | active |
