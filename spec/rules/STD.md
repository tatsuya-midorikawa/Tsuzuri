# STD: Standard library

The rules of this area, one sentence each, with the document that states them and the conformance tests that check them (`tests/conformance/STD/`). See `spec/README.md` for the format.

| ID | Rule | Doc | Tests | Status |
| --- | --- | --- | --- | --- |
| STD-001 | `Array.sum` MUST add the elements of an array. | [array.md](../../_tsuzuri/language-reference/built-in-types-and-modules/array.md) | STD-001-array-sum-must-add | active |
| STD-002 | `Array.sort` MUST return an ascending copy and MUST leave its argument unchanged. | [array.md](../../_tsuzuri/language-reference/built-in-types-and-modules/array.md) | STD-002-array-sort-must-return | active |
| STD-003 | `Array.try_item` MUST return `None` for an out-of-range index instead of trapping. | [array.md](../../_tsuzuri/language-reference/built-in-types-and-modules/array.md) | STD-003-array-try-item-must | active |
| STD-004 | `Array.update_at` MUST return an updated copy and MUST leave the original array unchanged. | [array.md](../../_tsuzuri/language-reference/built-in-types-and-modules/array.md) | STD-004-array-update-at-must | active |
| STD-005 | `Int.abs` of the minimum signed value MUST return the minimum value unchanged. | [int.md § 選択と絶対値](../../_tsuzuri/language-reference/built-in-types-and-modules/int.md#選択と絶対値) | STD-005-int-abs-of-minimum | active |
| STD-006 | `Int.checked_add` MUST return `None` on overflow and `Some` of the sum otherwise. | [exception-handling.md](../../_tsuzuri/language-reference/exception-handling/exception-handling.md) | STD-006-int-checked-add-must | active |
| STD-007 | `Array.binary_search` MUST return the index of a present element in a sorted array. | [array.md](../../_tsuzuri/language-reference/built-in-types-and-modules/array.md) | STD-007-array-binary-search-must | active |
| STD-008 | `Array.product` MUST multiply the elements of an array. | [array.md](../../_tsuzuri/language-reference/built-in-types-and-modules/array.md) | STD-008-array-product-must-multiply | active |
| STD-009 | `Parse.parse` at an integer type MUST return `Some` for the decimal text of a value in its range and `None` for any other text, including a number out of range. | [cast.md](../../_tsuzuri/language-reference/types-and-type-inference/cast.md) | STD-009-parse-parse-must-return | active |
| STD-010 | `Vec.push_owned` MUST append and `for` over a `Vec` MUST visit the elements in insertion order. | [for-in.md](../../_tsuzuri/language-reference/loops-and-conditionals/for-in.md) | STD-010-vec-push-owned-must | active |
