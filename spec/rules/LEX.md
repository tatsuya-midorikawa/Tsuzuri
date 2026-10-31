# LEX: Lexical

The rules of this area, one sentence each, with the document that states them and the conformance tests that check them (`tests/conformance/LEX/`). See `spec/README.md` for the format.

| ID | Rule | Doc | Tests | Status |
| --- | --- | --- | --- | --- |
| LEX-001 | A block comment MUST be allowed to nest. | [tokens.md](../../_tsuzuri/language-reference/values-and-functions/tokens.md) | LEX-001-block-comment-must-be | active |
| LEX-002 | A leading zero MUST NOT select octal: `0123` is the decimal value 123. | [literals.md](../../_tsuzuri/language-reference/literals-and-strings/literals.md) | LEX-002-leading-zero-must-not | active |
| LEX-003 | A digit separator `_` MUST be placed between digits only (E0001 otherwise). | [literals.md](../../_tsuzuri/language-reference/literals-and-strings/literals.md) | LEX-003-digit-separator-must-be | active |
| LEX-004 | A line comment `//` MUST run to the end of the line. | [language.md § ソースと宣言](../../docs/language.md#ソースと宣言) | LEX-004-line-comment-must-run | active |
| LEX-005 | A reserved word MUST NOT be used as an identifier, except that `union`, `yield` and `new` may be member names after a dot (`Set.union`). | [keywords.md](../../_tsuzuri/language-reference/values-and-functions/keywords.md) | LEX-005-reserved-word-must-not | active |
| LEX-006 | A contextual keyword such as `of` MUST remain usable as an identifier outside its syntax. | [keywords.md](../../_tsuzuri/language-reference/values-and-functions/keywords.md) | LEX-006-contextual-keyword-such-as | active |
| LEX-007 | `not` MUST be an ordinary function value, not a reserved word. | [keywords.md](../../_tsuzuri/language-reference/values-and-functions/keywords.md) | LEX-007-not-must-be-ordinary | active |
| LEX-008 | An unterminated string literal MUST be rejected (E0001). | [diagnostics.md](../../_tsuzuri/language-reference/compiler/diagnostics.md) | LEX-008-unterminated-string-literal-must | active |
| LEX-009 | An unknown escape sequence in a string literal MUST be rejected (E0001). | [literals.md § 注意点](../../_tsuzuri/language-reference/literals-and-strings/literals.md#注意点) | LEX-009-unknown-escape-sequence-in | active |
| LEX-010 | In the text of an interpolated string, a lone `}` outside a hole MUST be rejected (E0001). | [interpolated-strings.md](../../_tsuzuri/language-reference/literals-and-strings/interpolated-strings.md) | LEX-010-lone-outside-interpolation-hole | active |
