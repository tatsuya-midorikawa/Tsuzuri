# CTL: Control

The rules of this area, one sentence each, with the document that states them and the conformance tests that check them (`tests/conformance/CTL/`). See `spec/README.md` for the format.

| ID | Rule | Doc | Tests | Status |
| --- | --- | --- | --- | --- |
| CTL-001 | The condition of `if` MUST have type `bool`; there is no implicit conversion from numbers or strings. | [if.md](../../_tsuzuri/language-reference/loops-and-conditionals/if.md) | CTL-001-condition-of-if-must | active |
| CTL-002 | All branches of an `if` MUST have the same type. | [if.md](../../_tsuzuri/language-reference/loops-and-conditionals/if.md) | CTL-002-all-branches-of-if | active |
| CTL-003 | An `elif` chain MUST select the first branch whose condition is true. | [if.md](../../_tsuzuri/language-reference/loops-and-conditionals/if.md) | CTL-003-elif-chain-must-select | active |
| CTL-004 | `for x in a ..= b` MUST include both bounds. | [for-in.md](../../_tsuzuri/language-reference/loops-and-conditionals/for-in.md) | CTL-004-for-x-in-b | active |
| CTL-005 | `for x in a .. step ..= b` MUST visit a, a+step, ... while the value has not passed b, and a negative step MUST count down. | [for-in.md](../../_tsuzuri/language-reference/loops-and-conditionals/for-in.md) | CTL-005-for-x-in-step | active |
| CTL-006 | A range with a zero step MUST trap. | [for-in.md](../../_tsuzuri/language-reference/loops-and-conditionals/for-in.md) | CTL-006-range-with-zero-step | active |
| CTL-007 | `for` over an array MUST visit the elements in index order. | [for-in.md](../../_tsuzuri/language-reference/loops-and-conditionals/for-in.md) | CTL-007-for-over-array-must | active |
| CTL-008 | `for i = a to b` MUST include both bounds and run zero times when `a > b`; `i` has the integer type of the bounds (`i32` for literals without a suffix). | [for-to.md](../../_tsuzuri/language-reference/loops-and-conditionals/for-to.md) | CTL-008-for-i-to-b | active |
| CTL-009 | `continue` MUST skip to the next iteration and `break` MUST leave the innermost loop. | [for-in.md](../../_tsuzuri/language-reference/loops-and-conditionals/for-in.md) | CTL-009-continue-must-skip-to | active |
| CTL-010 | `break` outside a loop MUST be rejected (E1023). | [language.md § 制御構文](../../docs/language.md#制御構文) | CTL-010-break-outside-loop-must | active |
| CTL-011 | A `while` loop whose condition is initially false MUST run its body zero times. | [while.md](../../_tsuzuri/language-reference/loops-and-conditionals/while.md) | CTL-011-while-loop-whose-condition | active |
| CTL-012 | The condition of `while` MUST have type `bool`. | [while.md](../../_tsuzuri/language-reference/loops-and-conditionals/while.md) | CTL-012-condition-of-while-must | active |
| CTL-013 | `match` MUST try the arms in order and run the first arm whose pattern matches and whose guard holds. | [match.md](../../_tsuzuri/language-reference/pattern-matching/match.md) | CTL-013-match-must-try-arms | active |
| CTL-014 | A `match` that does not cover every value MUST be rejected (E1021). | [match.md](../../_tsuzuri/language-reference/pattern-matching/match.md) | CTL-014-match-that-does-not | active |
| CTL-015 | An arm with a `when` guard MUST NOT count towards exhaustiveness. | [match.md](../../_tsuzuri/language-reference/pattern-matching/match.md) | CTL-015-arm-with-when-guard | active |
| CTL-016 | An arm fully covered by earlier arms MUST produce warning W1003. | [match.md](../../_tsuzuri/language-reference/pattern-matching/match.md) | CTL-016-arm-fully-covered-by | active |
| CTL-017 | All arms of a `match` MUST have the same type. | [match.md](../../_tsuzuri/language-reference/pattern-matching/match.md) | CTL-017-all-arms-of-match | active |
| CTL-018 | A union pattern MUST bind the payload of the matched case. | [pattern-matching.md](../../_tsuzuri/language-reference/pattern-matching/pattern-matching.md) | CTL-018-union-pattern-must-bind | active |
| CTL-019 | An or-pattern MUST match when any alternative matches and every alternative MUST bind the same names. | [pattern-matching.md](../../_tsuzuri/language-reference/pattern-matching/pattern-matching.md) | CTL-019-or-pattern-must-match | active |
| CTL-020 | A record pattern MUST bind fields by name and MAY ignore the others with `_`. | [pattern-matching.md](../../_tsuzuri/language-reference/pattern-matching/pattern-matching.md) | CTL-020-record-pattern-must-bind | active |
| CTL-021 | A tuple pattern in `for` MUST destructure each element. | [pattern-matching.md](../../_tsuzuri/language-reference/pattern-matching/pattern-matching.md) | CTL-021-tuple-pattern-in-for | active |
| CTL-022 | A refutable pattern in `for` MUST NOT be checked for exhaustiveness and a non-matching element MUST trap. | [for-in.md](../../_tsuzuri/language-reference/loops-and-conditionals/for-in.md) | CTL-022-refutable-pattern-in-for | active |
| CTL-023 | A failed `assert` MUST trap and MUST NOT be caught by `try`. | [exception-handling.md](../../_tsuzuri/language-reference/exception-handling/exception-handling.md) | CTL-023-failed-assert-must-trap | active |
| CTL-024 | `try` MUST NOT catch traps such as division by zero. | [exception-handling.md](../../_tsuzuri/language-reference/exception-handling/exception-handling.md) | CTL-024-try-must-not-catch | active |
| CTL-025 | `let!` in a `Maybe` computation MUST stop at the first `None` and return `None`. | [exception-handling.md](../../_tsuzuri/language-reference/exception-handling/exception-handling.md) | CTL-025-let-in-maybe-computation | active |
| CTL-026 | `let!` in a `Result` computation MUST stop at the first `Error` and return it. | [exception-handling.md](../../_tsuzuri/language-reference/exception-handling/exception-handling.md) | CTL-026-let-in-result-computation | active |
