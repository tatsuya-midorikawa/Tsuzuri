# TYP: Types

The rules of this area, one sentence each, with the document that states them and the conformance tests that check them (`tests/conformance/TYP/`). See `spec/README.md` for the format.

| ID | Rule | Doc | Tests | Status |
| --- | --- | --- | --- | --- |
| TYP-001 | A value whose type differs from the annotated type of its binding MUST be rejected (E1003); there is no implicit conversion. | [language.md § 型とメモリ](../../docs/language.md#型とメモリ) | TYP-001-value-whose-type-differs | active |
| TYP-002 | Records MUST be nominal: two record types with identical fields MUST be different types. | [language.md § 型とメモリ](../../docs/language.md#型とメモリ) | TYP-002-records-must-be-nominal | active |
| TYP-003 | A function type MUST be right-associative and functions MUST be curried, so partial application yields a function. | [language.md § 型とメモリ](../../docs/language.md#型とメモリ) | TYP-003-function-type-must-be | active |
| TYP-004 | An unsuffixed integer literal MUST take its type from a later use, in both directions. | [language.md § 型とメモリ](../../docs/language.md#型とメモリ) | TYP-004-unsuffixed-integer-literal-must | active |
| TYP-005 | A decimal literal without any typing context MUST have type `f64`. | [language.md § 型とメモリ](../../docs/language.md#型とメモリ) | TYP-005-decimal-literal-without-any | active |
| TYP-006 | A `let` whose type cannot be determined MUST be rejected; an empty array needs a type annotation (E1004). | [language.md § 型とメモリ](../../docs/language.md#型とメモリ) | TYP-006-let-whose-type-cannot | active |
| TYP-007 | A generic function MUST be usable at several types in one program. | [language.md § ジェネリックなレコード](../../docs/language.md#ジェネリックなレコード) | TYP-007-generic-function-must-be | active |
| TYP-008 | A constraint used by a generic body MUST be inferred from the operators it uses, so `x + x` requires `Add`. | [language.md § 型とメモリ](../../docs/language.md#型とメモリ) | TYP-008-constraint-used-by-generic | active |
| TYP-009 | A generic function MUST be rejected at a type that lacks the required instance (E1005). | [language.md § 多相関数と型クラス](../../docs/language.md#多相関数と型クラス) | TYP-009-generic-function-must-be | active |
| TYP-010 | A declared type parameter MUST be used by a field or a payload (E1024). | [language.md § ジェネリックなレコード](../../docs/language.md#ジェネリックなレコード) | TYP-010-declared-type-parameter-must | active |
| TYP-011 | A generic record literal MUST infer its type arguments from the field values. | [language.md § 型とメモリ](../../docs/language.md#型とメモリ) | TYP-011-generic-record-literal-must | active |
| TYP-012 | A type alias MUST be interchangeable with its target type. | [language.md § 型別名](../../docs/language.md#型別名) | TYP-012-type-alias-must-be | active |
| TYP-013 | An assignment expression (`=` or `+=`, `-=`, `*=`, `/=`, `%=`) MUST have type `unit`. | [language.md § ソースと宣言](../../docs/language.md#ソースと宣言) | TYP-013-assignment-expression-must-have | active |
| TYP-014 | A record without a derived or declared `Eq` instance MUST NOT support `==` (E1005). | [language.md § 多相関数と型クラス](../../docs/language.md#多相関数と型クラス) | TYP-014-record-without-derived-or | active |
| TYP-015 | `deriving (Eq)` MUST give a record structural equality. | [language.md § 多相関数と型クラス](../../docs/language.md#多相関数と型クラス) | TYP-015-deriving-eq-must-give | active |
| TYP-016 | A cast between `char` and an integer with `as` MUST be rejected (E1005); `Char.to_u16` and `Char.of_u16` MUST be used instead. | [language.md § 数値仕様](../../docs/language.md#数値仕様) | TYP-016-cast-between-char-and | active |
| TYP-017 | A recursive record that contains itself without indirection MUST be rejected (E1010). | [language.md § 再帰的なデータ型](../../docs/language.md#再帰的なデータ型) | TYP-017-recursive-record-that-contains | active |
| TYP-018 | A union case constructor MUST check its payload type (E1003). | [language.md § 共用体（union）](../../docs/language.md#共用体union) | TYP-018-union-case-constructor-must | active |
| TYP-019 | A `let` MAY shadow a name in the same scope, including with a different type. | [language.md § ソースと宣言](../../docs/language.md#ソースと宣言) | TYP-019-let-may-shadow-name | active |
| TYP-020 | An unknown type name MUST be rejected at compile time. | [language.md § ジェネリックなレコード](../../docs/language.md#ジェネリックなレコード) | TYP-020-unknown-type-name-must | active |
| TYP-021 | A type class instance MUST be unique per class and type (E1016). | [language.md § 多相関数と型クラス](../../docs/language.md#多相関数と型クラス) | TYP-021-type-class-instance-must | active |
| TYP-022 | A class declared in a `.tt` file MUST be usable by an instance in a `.tz` file, and the method type MUST come from the class. | [language.md § 多相関数と型クラス](../../docs/language.md#多相関数と型クラス) | TYP-022-class-declared-in-tt | active |
| TYP-023 | A user instance of `Index`, `IndexMut`, `AddAssign`..`RemAssign`, or an arithmetic operator class MUST have at least one dispatch type head declared in this program (E1016). | [language.md § 多相関数と型クラス](../../docs/language.md#多相関数と型クラス) | TYP-023-orphan-instance-extension, TYP-023-orphan-instance-rule | active |
| TYP-024 | An operand of type `ref T` or `ref mut T` with a known head of `T` MUST be typed as `T` when the reference type fails (E1003, E1005); borrowing operators borrow the reference, and value-taking operators read as `deref`. | [language.md § 式と評価順序](../../docs/language.md#式と評価順序) | TYP-024-reference-operator-operands | active |
| TYP-025 | A multi-parameter arithmetic operator instance MUST name a non-builtin record, union, or extern type on an operand side, MUST NOT repeat a same-type header, and MUST cover any bare operand type variable in the other operand (E1016). | [language.md § 多相関数と型クラス](../../docs/language.md#多相関数と型クラス) | TYP-025-operator-extension-form | active |
| TYP-026 | An arithmetic operator expression whose operands do not match a same-type instance MUST resolve against the unified operator table and infer unsuffixed numeric literals from the matching operand side when unique (E1043 when ambiguous). | [language.md § 多相関数と型クラス](../../docs/language.md#多相関数と型クラス) | TYP-026-operator-extension-resolution | active |
