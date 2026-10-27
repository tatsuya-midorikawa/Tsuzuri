# Pitfalls

Each line is a wrong form, the code that `tsuzuri check` reports, and the right form. All are checked against this compiler. `tsuzuri explain CODE` explains a code.

## Forms from other languages
- `IO.writeln s` as a statement is E1003 (`expected unit, found IO<unit>`): `do! IO.writeln s`.
- `else if` on a new line is E0002: `elif`.
- `n + 1.5` and `"a" + 1` are E1003: `n as f64 + 1.5`, `$"a{n}"`.
- `Vec.empty ()` is E1006: `Vec.empty()`. `main` must return `i32` (E1003).
- `u8` and `usize` are E1004: `i8u`, `i64`. `String` is E1004: `string`.
<!--if edition=2026-->
- `p.x = 1` and `xs[i] = v` are E1012 (fields and elements are immutable): `{ p with x = 1 }`, `xs = Array.update_at i v xs`.
<!--endif-->
<!--if edition=2027-->
- `p.x = 1` and `xs[i] = v` need a `let mut` owner or a `mut T` parameter (else E1014).
<!--endif-->
- `for i in 0 .. xs.length` compiles, then traps: `..` includes the end. Use `0 .. xs.length - 1`.

## Hints the compiler adds
For these mistakes the diagnostic carries the hint below as `help` (code, then the hint):
<!-- generated:pitfalls -->
<!-- /generated:pitfalls -->
