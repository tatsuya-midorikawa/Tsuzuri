# SYN: Syntax

The rules of this area, one sentence each, with the document that states them and the conformance tests that check them (`tests/conformance/SYN/`). See `spec/README.md` for the format.

| ID | Rule | Doc | Tests | Status |
| --- | --- | --- | --- | --- |
| SYN-001 | Top-level statements MUST be separable by a newline or by `;`. | [language.md § アプリケーションのエントリーポイント](../../docs/language.md#アプリケーションのエントリーポイント) | SYN-001-top-level-statements-must | active |
| SYN-002 | Inside a brace block, a `let` binding and the expression that follows it on the same line MUST be separated by `;` (E0002). | [language.md § アプリケーションのエントリーポイント](../../docs/language.md#アプリケーションのエントリーポイント) | SYN-002-statements-inside-brace-block | active |
| SYN-003 | A brace block and an indented block MUST be equivalent. | [statements.md](../../_tsuzuri/language-reference/values-and-functions/statements.md) | SYN-003-brace-block-and-indented | active |
| SYN-004 | A line that starts with `\|>` MUST continue the previous line. | [language.md § アプリケーションのエントリーポイント](../../docs/language.md#アプリケーションのエントリーポイント) | SYN-004-line-that-starts-with | active |
| SYN-005 | At the top level, a line that starts with `(` after a completed `let` or expression MUST start a new expression, not continue the previous line. | [language.md § アプリケーションのエントリーポイント](../../docs/language.md#アプリケーションのエントリーポイント) | SYN-005-line-that-starts-with | active |
| SYN-006 | The operand of `deref` MUST be a single term; `deref f x` MUST be rejected (E0002). | [language.md § 呼び出し引数の暗黙の借用](../../docs/language.md#呼び出し引数の暗黙の借用) | SYN-006-operand-of-deref-must | active |
| SYN-007 | Expression nesting deeper than 128 levels MUST be rejected (E0002). | [statements.md](../../_tsuzuri/language-reference/values-and-functions/statements.md) | SYN-007-expression-nesting-deeper-than | active |
| SYN-008 | `fn main` without a preceding `def` signature MUST be rejected (E0002). | [language.md § アプリケーションのエントリーポイント](../../docs/language.md#アプリケーションのエントリーポイント) | SYN-008-fn-main-without-preceding | active |
| SYN-009 | In `Main.tz`, an entry point `def main` with a signature other than `unit -> i32` or `[string] -> i32` MUST be rejected (E2004); a `main` of another module is an ordinary function. | [language.md § アプリケーションのエントリーポイント](../../docs/language.md#アプリケーションのエントリーポイント) | SYN-009-main-with-signature-other | active |
| SYN-010 | An old-style generic record `Pair 'a 'b` MUST be rejected (E0002); type arguments use `<...>`. | [generics.md](../../_tsuzuri/language-reference/types-and-type-inference/generics.md) | SYN-010-old-style-generic-record | active |
