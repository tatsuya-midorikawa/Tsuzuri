# EVL: Evaluation order

The rules of this area, one sentence each, with the document that states them and the conformance tests that check them (`tests/conformance/EVL/`). See `spec/README.md` for the format.

| ID | Rule | Doc | Tests | Status |
| --- | --- | --- | --- | --- |
| EVL-001 | The operands of a binary operator MUST be evaluated left to right in source order, even when precedence groups the operators differently. | [language.md § 式と評価順序](../../docs/language.md#式と評価順序) | EVL-001-operands-of-binary-operator | active |
| EVL-002 | The arguments of a call MUST be evaluated left to right. | [language.md § 呼び出し引数の暗黙の借用](../../docs/language.md#呼び出し引数の暗黙の借用) | EVL-002-arguments-of-call-must | active |
| EVL-003 | All arguments MUST be evaluated before the callee body runs. | [language.md § 呼び出し引数の暗黙の借用](../../docs/language.md#呼び出し引数の暗黙の借用) | EVL-003-all-arguments-must-be | active |
| EVL-004 | `&&` MUST NOT evaluate its right operand when the left operand is false. | [language.md § 式と評価順序](../../docs/language.md#式と評価順序) | EVL-004-must-not-evaluate-its | active |
| EVL-005 | `\|\|` MUST NOT evaluate its right operand when the left operand is true. | [language.md § 式と評価順序](../../docs/language.md#式と評価順序) | EVL-005-must-not-evaluate-its | active |
| EVL-006 | A trapping operation in a short-circuited operand MUST NOT execute. | [language.md § 式と評価順序](../../docs/language.md#式と評価順序) | EVL-006-trapping-operation-in-short | active |
| EVL-007 | `if` MUST evaluate only the selected branch. | [language.md § if / then / else](../../docs/language.md#if--then--else) | EVL-007-if-must-evaluate-only | active |
| EVL-008 | `match` MUST evaluate its scrutinee exactly once, whatever the number of arms tried. | [language.md § match と分解](../../docs/language.md#match-と分解) | EVL-008-match-must-evaluate-its | active |
| EVL-009 | The elements of an array literal MUST be evaluated in written order. | [language.md § 式と評価順序](../../docs/language.md#式と評価順序) | EVL-009-elements-of-array-literal | active |
| EVL-010 | The fields of a record literal MUST be evaluated in written order, not in declaration order. | [language.md § 式と評価順序](../../docs/language.md#式と評価順序) | EVL-010-fields-of-record-literal | active |
| EVL-011 | The elements of a tuple MUST be evaluated left to right. | [language.md § 式と評価順序](../../docs/language.md#式と評価順序) | EVL-011-elements-of-tuple-must | active |
| EVL-012 | An assignment MUST evaluate its right-hand side before it determines the target place. | [language.md § 呼び出し引数の暗黙の借用](../../docs/language.md#呼び出し引数の暗黙の借用) | EVL-012-assignment-must-evaluate-its | active |
| EVL-013 | The bounds of `for i = a to b` MUST each be evaluated exactly once, left to right, before the first iteration. | [language.md § for / while](../../docs/language.md#for--while) | EVL-013-bounds-of-for-i | active |
| EVL-014 | A `while` condition MUST be evaluated before every iteration, including the final failing check. | [language.md § for / while](../../docs/language.md#for--while) | EVL-014-while-condition-must-be | active |
| EVL-015 | `x \|> f \|> g` MUST evaluate `x` first and then apply `f` and `g` in order. | [language.md § 式と評価順序](../../docs/language.md#式と評価順序) | EVL-015-x-f-g-must | active |
| EVL-016 | A function-valued expression MUST be evaluated before its arguments. | [language.md § 式と評価順序](../../docs/language.md#式と評価順序) | EVL-016-function-valued-expression-must | active |
| EVL-017 | The holes of an interpolated string MUST be evaluated left to right. | [language.md § string と utf8string](../../docs/language.md#string-と-utf8string) | EVL-017-holes-of-interpolated-string | active |
| EVL-018 | Binary operators other than `**` MUST be left-associative. | [language.md § 式と評価順序](../../docs/language.md#式と評価順序) | EVL-018-binary-operators-other-than | active |
| EVL-019 | An operation that traps MUST stop the program at its own evaluation point: earlier side effects are visible and later operands are not evaluated. | [language.md § 検査付き算術と例外](../../docs/language.md#検査付き算術と例外) | EVL-019-operation-that-traps-must | active |
| EVL-020 | A chain `a && b && c` MUST stop at the first false operand. | [language.md § 式と評価順序](../../docs/language.md#式と評価順序) | EVL-020-chain-b-c-must | active |
| EVL-021 | An index expression MUST evaluate its base before its key components and the components left to right, before any bounds check or instance call; as the target of a plain assignment `=` it does so after the right-hand side. | [language.md § 式と評価順序](../../docs/language.md#式と評価順序) | EVL-021-index-evaluates-base-before-key | active |
