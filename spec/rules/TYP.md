# TYP: Types

The rules of this area, one sentence each, with the document that states them and the conformance tests that check them (`tests/conformance/TYP/`). See `spec/README.md` for the format.

| ID | Rule | Doc | Tests | Status |
| --- | --- | --- | --- | --- |
| TYP-001 | A value whose type differs from the annotated type of its binding MUST be rejected (E1003); there is no implicit conversion. | [types.md](../../_tsuzuri/language-reference/types-and-type-inference/types.md) | TYP-001-value-whose-type-differs | active |
| TYP-002 | Records MUST be nominal: two record types with identical fields MUST be different types. | [types.md](../../_tsuzuri/language-reference/types-and-type-inference/types.md) | TYP-002-records-must-be-nominal | active |
| TYP-003 | A function type MUST be right-associative and functions MUST be curried, so partial application yields a function. | [types.md](../../_tsuzuri/language-reference/types-and-type-inference/types.md) | TYP-003-function-type-must-be | active |
| TYP-004 | An unsuffixed integer literal MUST take its type from a later use, in both directions. | [type-inference.md](../../_tsuzuri/language-reference/types-and-type-inference/type-inference.md) | TYP-004-unsuffixed-integer-literal-must | active |
| TYP-005 | A decimal literal without any typing context MUST have type `f64`. | [type-inference.md](../../_tsuzuri/language-reference/types-and-type-inference/type-inference.md) | TYP-005-decimal-literal-without-any | active |
| TYP-006 | A `let` whose type cannot be determined MUST be rejected; an empty array needs a type annotation (E1004). | [type-inference.md](../../_tsuzuri/language-reference/types-and-type-inference/type-inference.md) | TYP-006-let-whose-type-cannot | active |
| TYP-007 | A generic function MUST be usable at several types in one program. | [generics.md](../../_tsuzuri/language-reference/types-and-type-inference/generics.md) | TYP-007-generic-function-must-be | active |
| TYP-008 | A constraint used by a generic body MUST be inferred from the operators it uses, so `x + x` requires `Add`. | [type-inference.md](../../_tsuzuri/language-reference/types-and-type-inference/type-inference.md) | TYP-008-constraint-used-by-generic | active |
| TYP-009 | A generic function MUST be rejected at a type that lacks the required instance (E1005). | [constraints.md](../../_tsuzuri/language-reference/types-and-type-inference/constraints.md) | TYP-009-generic-function-must-be | active |
| TYP-010 | A declared type parameter MUST be used by a field or a payload (E1024). | [generics.md](../../_tsuzuri/language-reference/types-and-type-inference/generics.md) | TYP-010-declared-type-parameter-must | active |
| TYP-011 | A generic record literal MUST infer its type arguments from the field values. | [type-inference.md](../../_tsuzuri/language-reference/types-and-type-inference/type-inference.md) | TYP-011-generic-record-literal-must | active |
| TYP-012 | A type alias MUST be interchangeable with its target type. | [alias.md](../../_tsuzuri/language-reference/types-and-type-inference/alias.md) | TYP-012-type-alias-must-be | active |
| TYP-013 | An assignment expression MUST have type `unit`. | [values.md](../../_tsuzuri/language-reference/values-and-functions/values.md) | TYP-013-assignment-expression-must-have | active |
| TYP-014 | A record without a derived or declared `Eq` instance MUST NOT support `==` (E1005). | [type-classes.md](../../_tsuzuri/language-reference/types-and-type-inference/type-classes.md) | TYP-014-record-without-derived-or | active |
| TYP-015 | `deriving (Eq)` MUST give a record structural equality. | [type-classes.md](../../_tsuzuri/language-reference/types-and-type-inference/type-classes.md) | TYP-015-deriving-eq-must-give | active |
| TYP-016 | A cast between `char` and an integer with `as` MUST be rejected (E1005); `Char.to_u16` and `Char.of_u16` MUST be used instead. | [cast.md](../../_tsuzuri/language-reference/types-and-type-inference/cast.md) | TYP-016-cast-between-char-and | active |
| TYP-017 | A recursive record that contains itself without indirection MUST be rejected (E1010). | [language.md § 再帰的なデータ型](../../docs/language.md#再帰的なデータ型) | TYP-017-recursive-record-that-contains | active |
| TYP-018 | A union case constructor MUST check its payload type (E1003). | [types.md](../../_tsuzuri/language-reference/types-and-type-inference/types.md) | TYP-018-union-case-constructor-must | active |
| TYP-019 | A `let` MAY shadow a name in the same scope, including with a different type. | [values.md](../../_tsuzuri/language-reference/values-and-functions/values.md) | TYP-019-let-may-shadow-name | active |
| TYP-020 | An unknown type name MUST be rejected at compile time. | [generics.md](../../_tsuzuri/language-reference/types-and-type-inference/generics.md) | TYP-020-unknown-type-name-must | active |
| TYP-021 | A type class instance MUST be unique per class and type (E1016). | [type-classes.md](../../_tsuzuri/language-reference/types-and-type-inference/type-classes.md) | TYP-021-type-class-instance-must | active |
| TYP-022 | A class declared in a `.tt` file MUST be usable by an instance in a `.tz` file, and the method type MUST come from the class. | [type-classes.md](../../_tsuzuri/language-reference/types-and-type-inference/type-classes.md) | TYP-022-class-declared-in-tt | active |
