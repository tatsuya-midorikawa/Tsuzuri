# STD: Standard library

The rules of this area, one sentence each, with the document that states them and the conformance tests that check them (`tests/conformance/STD/`). See `spec/README.md` for the format.

| ID | Rule | Doc | Tests | Status |
| --- | --- | --- | --- | --- |
| STD-001 | `Array.sum` MUST add the elements of an array. | [language.md § 配列・リスト API](../../docs/language.md#配列リスト-api) | STD-001-array-sum-must-add | active |
| STD-002 | `Array.sort` MUST return an ascending copy and MUST leave its argument unchanged. | [language.md § 配列・リスト API](../../docs/language.md#配列リスト-api) | STD-002-array-sort-must-return | active |
| STD-003 | `Array.try_item` MUST return `None` for an out-of-range index instead of trapping. | [language.md § 配列・リスト API](../../docs/language.md#配列リスト-api) | STD-003-array-try-item-must | active |
| STD-004 | `Array.update_at` MUST return an updated copy and MUST leave the original array unchanged. | [language.md § 配列・リスト API](../../docs/language.md#配列リスト-api) | STD-004-array-update-at-must | active |
| STD-005 | `Int.abs` of the minimum signed value MUST return the minimum value unchanged. | [language.md § 整数 API](../../docs/language.md#整数-api) | STD-005-int-abs-of-minimum | active |
| STD-006 | `Int.checked_add` MUST return `None` on overflow and `Some` of the sum otherwise. | [language.md § 検査付き算術と例外](../../docs/language.md#検査付き算術と例外) | STD-006-int-checked-add-must | active |
| STD-007 | `Array.binary_search` MUST return the index of a present element in a sorted array. | [language.md § 配列・リスト API](../../docs/language.md#配列リスト-api) | STD-007-array-binary-search-must | active |
| STD-008 | `Array.product` MUST multiply the elements of an array. | [language.md § 配列・リスト API](../../docs/language.md#配列リスト-api) | STD-008-array-product-must-multiply | active |
| STD-009 | `Parse.parse` at `i64` MUST return `Some` for the decimal text of a value in range, and `None` for text that is not a number or is a number out of range. | [language.md § 数値仕様](../../docs/language.md#数値仕様) | STD-009-parse-parse-must-return | active |
| STD-010 | `Vec.push_owned` MUST append and `for` over a `Vec` MUST visit the elements in insertion order. | [language.md § for / while](../../docs/language.md#for--while) | STD-010-vec-push-owned-must | active |
