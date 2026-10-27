# G23 付録: 発行箇所の表 — 意味解析・所有権・多相・GPU・葉のモジュール

[G23 本体](G23-diagnostic-kinds-explain-hints.md) の付録 C です。HEAD `9dd0fbbc`（`src/` は `a2313d19` と同一）の `src/` を、`grep` と構文を見る補助スクリプトで数え、一行ずつ人が kind を決めました。

列: `#` はファイル内の通し番号（ソースの順）、`関数` は発行箇所を含む関数（`型::関数`）、`形` は呼び出しの形（下の表）、`コード` は元のコード、`kind` は移行後の kind、`メッセージ` は元のメッセージの先頭（`{}` は書式の穴。`<<…>>` は変数）。

| 形 | 意味 | 直し方（コード・メッセージ・スパンは変えない） |
| --- | --- | --- |
| `new` | `Diagnostic::new(code, …)` を直接呼ぶ | `Diagnostic::of(DiagnosticKind::X, …)` |
| `warn` | `Diagnostic::warning(code, …)` | `Diagnostic::of(DiagnosticKind::X, …)`（重要度は kind から決まる） |
| `drv` | `driver_error(code, …)`（`src/driver.rs`） | `driver_error_of(DiagnosticKind::X, …)`（手順 2.15 で足す） |
| `own` | `error(code, …)`（`src/ownership.rs`） | `error(DiagnosticKind::X, …)`（`error` の第 1 引数の型を `DiagnosticKind` にし、本体を `Diagnostic::of` に） |
| `loc` | `Location::error(code, …)`（`src/fetch.rs`） | `location.error(DiagnosticKind::X, …)`（第 1 引数の型を変える） |
| `clo` | 局所の closure の呼び出し | closure の第 1 引数を `DiagnosticKind` にして呼び出しに kind を足す |
| `par` | `self.error(message)`（コード `E0002` を内側に持つ） | `self.error_of(DiagnosticKind::X, message)`（手順 2.3 で足す） |
| `rec` | `self.rec_error(message, span)` | `self.rec_error_of(DiagnosticKind::X, message, span)` |
| `lin` | `Line::error(message)`（マニフェスト。`E0002`） | `error(DiagnosticKind::X, message)`（kind 引数を足す） |
| `fail` | `failure(span, message)`（コードを内側に持つ） | `failure(DiagnosticKind::X, span, message)` |
| `uns` | `unsupported(message, span)`（GPU。`E1018`） | `unsupported(DiagnosticKind::X, message, span)` |
| `br` | 1 つの発行箇所の分岐ごとの kind | 分岐を `(DiagnosticKind, message)` の組にして最後に `Diagnostic::of`（付録 E.2） |
| `man` | `Diagnostic` を作らない文字列のエラーなど、手で見つけた箇所 | 付録 E.2 の該当項目 |

### `src/check.rs`（139 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | NameInfo::private_error | new | E1022 | private_name_access | private {kind} '{name}' is only visible inside module '{}' |
| 2 | Names::check_module | new | E1004 | ambiguous_name | '{name}' is ambiguous: using declarations make the modules |
| 3 | Names::repeated_module | new | E1004 | repeated_module_name | '{path}' repeats the module name; the {kind} '{name}' shar |
| 4 | Names::ambiguous_alias | new | E1004 | ambiguous_builder_alias | builder alias '{written}' is ambiguous: the builders {buil |
| 5 | Names::type_head | new | E1004 | ambiguous_name | '{}' names both a type class and the {} type '{}'; qualify |
| 6 | Names::type_head | new | E1004 | unknown_type_name | unknown type class, record type, or union type '{}'; {} |
| 7 | Names::record | new | E1004 | not_a_record_type | '{name}' names the union type '{}', not a record type; con |
| 8 | Names::record | new | E1004 | not_a_record_type | '{}' is a type alias; use the original record name to cons |
| 9 | Names::record | new | E1004 | not_a_record_type | '{name}' names the extern type '{}', not a record type; ho |
| 10 | Names::named_type | new | E1004 | unknown_type_name | unknown record, union, or type alias '{name}'; join namesp |
| 11 | Names::type_result | new | E1004 | unknown_type_name | unknown record, union, or type alias '{name}' |
| 12 | Names::type_result | new | E1004 | ambiguous_name | ambiguous {kind} '{name}'; qualify it as {} |
| 13 | Names::class_result | new | E1004 | ambiguous_name | ambiguous type class '{name}'; qualify it as {} |
| 14 | Names::std_type | new | E1004 | std_item_missing | this builtin needs the standard library type '{qualified}' |
| 15 | Names::case | new | E1004 | ambiguous_name | ambiguous union case '{name}'; qualify it as {} |
| 16 | Names::union_case | new | E1002 | unknown_union_case | union '{}' has no case '{name}' |
| 17 | Names::member_function | new | E1005 | function_constraint_unresolvable | {} has no declaring module for function constraint '#{name |
| 18 | Names::member_function | new | E1005 | function_constraint_unsatisfied | {} does not satisfy '#{name}': module '{}' has no function |
| 19 | Names::active_pattern | new | E1004 | ambiguous_name | ambiguous active pattern '{name}'; qualify one of {} |
| 20 | validate_public_type | new | E1022 | private_type_in_public_api | private type '{}' leaks from public {kind} '{owner}'; make |
| 21 | validate_public_type | new | E1022 | private_type_in_public_api | private type '{}' leaks from public {kind} '{owner}'; make |
| 22 | check_modules_collect | new | E1011 | reserved_std_namespace | <<message>> |
| 23 | check_modules_collect | new | E1017 | module_path_limit | module paths are limited to 16 segments and 255 bytes |
| 24 | check_modules_collect | new | E1011 | invalid_module_name | module name '{stem}' must start with an uppercase ASCII le |
| 25 | check_modules_collect | new | E1011 | reserved_std_namespace | module '{}' is in the namespace 'std', which is reserved f |
| 26 | check_modules_collect | new | E1011 | duplicate_module | module '{}' is declared by more than one file; rename a fi |
| 27 | check_modules_collect | new | E1011 | invalid_module_name | invalid or duplicate module name '{name}'; each .tz, .tt,  |
| 28 | check_modules_collect | new | E1011 | invalid_using_declaration | <<message>> |
| 29 | check_modules_collect | new | E1008 | extern_declaration_constraints | extern declarations cannot have constraints or named regio |
| 30 | check_modules_collect | new | E1018 | std_declares_test | embedded standard-library sources cannot declare tests |
| 31 | check_modules_collect | new | E1018 | std_declares_test | embedded standard-library sources cannot declare benchmark |
| 32 | check_modules_collect | new | E1024 | type_name_case | a type alias name must start with an uppercase ASCII lette |
| 33 | check_modules_collect | new | E1024 | undeclared_type_variable | type variable '{variable} is not declared by type alias '{ |
| 34 | check_modules_collect | new | E1024 | unused_type_parameter | {} is not used by the alias target; remove it or use it in |
| 35 | check_modules_collect | new | E1024 | undeclared_type_variable | type variable '{variable} is not declared by record '{}';  |
| 36 | check_modules_collect | new | E1013 | mutable_record_field | record fields are immutable; use an owned update instead o |
| 37 | check_modules_collect | new | E1024 | unused_type_parameter | {} is not used by any field; remove it or add a field that |
| 38 | check_modules_collect | new | E1018 | std_exports_function | the standard library cannot export functions; std modules  |
| 39 | check_modules_collect | new | E1008 | export_active_recognizer | active recognizers cannot be exported; expose an ordinary  |
| 40 | check_modules_collect | new | E1001 | duplicate_export_name | duplicate export 'tz_{}'; C/WASM export names must be uniq |
| 41 | check_modules_collect | new | E1008 | unsupported_export_signature | exports support scalar values, borrowed i64/f64/ubyte arra |
| 42 | check_modules_collect | new | E2004 | invalid_main_signature | the entry point must be 'def main :: unit -> i32' or 'def  |
| 43 | check_modules_collect | new | E1015 | extern_signature_not_concrete | extern signatures must be concrete; provide a scalar ABI t |
| 44 | check_modules_collect | new | E1008 | unsupported_extern_signature | extern callback parameters support functions over scalar,  |
| 45 | check_modules_collect | new | E1008 | unsupported_extern_signature | extern signatures support scalar/unit, shared ABI buffers  |
| 46 | check_modules_collect | new | E1008 | conflicting_extern_declaration | extern symbol '{}' is declared with different signatures o |
| 47 | check_modules_collect | man | E1026 | constant_type_not_concrete | constant types must be concrete; remove type variables |
| 48 | check_modules_collect | new | E1015 | type_variable_absent_from_signature | constraint mentions a type variable absent from the signat |
| 49 | check_modules_collect | new | E1015 | type_variable_absent_from_signature | the function constraint's type mentions a type variable ab |
| 50 | check_modules_collect | new | E1006 | active_recognizer_parameters | an active recognizer needs an input parameter |
| 51 | check_modules_collect | new | E1003 | active_recognizer_result | a partial active recognizer must return bool or Maybe payl |
| 52 | check_modules_collect | new | E1020 | active_pattern_case_count | active pattern and backing union must have the same case c |
| 53 | check_modules_collect | new | E1003 | active_recognizer_result | a multi-case active recognizer must return a union |
| 54 | check_modules_collect | new | E2004 | toplevel_code_outside_main | top-level execution is only allowed in Main.tz; other modu |
| 55 | check_modules_collect | new | E2004 | ambiguous_entry_point | Main.tz must use either top-level entry-point code or 'def |
| 56 | validate_cpu_functions | new | E1005 | cpu_function_wide_vector_boundary | a '@cpu' function cannot take or return a SIMD vector wide |
| 57 | validate_cpu_functions | new | E1005 | cpu_function_wide_vector_boundary | a '@cpu' function cannot call a function value that takes  |
| 58 | validate_callbacks | new | E1008 | extern_callback_call_form | extern '{}' takes a callback and must be called directly w |
| 59 | validate_callbacks | new | E1008 | extern_callback_argument | callback arguments must name a top-level user function wit |
| 60 | infer_active_result | new | E1020 | reserved_active_result_variable | 'T is reserved for the implicit active-pattern result, not |
| 61 | duplicate | new | E1001 | duplicate_name | duplicate or reserved name '{}' |
| 62 | declared_parameters | new | E1024 | invalid_type_parameter_name | '_ cannot name a {kind} type parameter; use a variable suc |
| 63 | declared_parameters | new | E1024 | duplicate_type_parameter | duplicate {} in {kind} '{name}'; give each parameter a dis |
| 64 | undeclared_length | new | E1024 | undeclared_length_parameter | length '{length}' is not declared by {kind} '{name}'; add  |
| 65 | collect_unions | new | E1024 | type_name_case | union type '{}' must start with an uppercase ASCII letter |
| 66 | collect_unions | new | E1024 | union_case_name_case | union case '{}' must start with an uppercase ASCII letter; |
| 67 | collect_unions | new | E1024 | duplicate_union_case | duplicate case '{}' in union '{}'; give each case a distin |
| 68 | check_union | new | E1024 | undeclared_type_variable | type variable '{variable} is not declared by union '{name} |
| 69 | check_union | new | E1013 | borrowed_union_payload | union payloads are owned values; borrowed payloads require |
| 70 | check_union | new | E1024 | unused_type_parameter | {} is not used by any case payload; remove it or add a pay |
| 71 | resolve_type_with_kinds | new | E1015 | constructor_variable_kind_annotation | type constructor variable requires a matching explicit cla |
| 72 | resolve_type_with_kinds | new | E1004 | type_argument_count | Vec expects exactly one element type |
| 73 | resolve_type_with_kinds | new | E1004 | type_argument_count | {} expects exactly one value type, as in '{}<i64>' |
| 74 | resolve_type_with_kinds | new | E1004 | type_argument_count | {} expects exactly one value type, as in '{}<i64>' |
| 75 | resolve_type_with_kinds | man | E1015 | missing_type_arguments | recode: error.code = "E1015" for a non-alias named type us |
| 76 | resolve_type_with_kinds | new | E1004 | type_argument_count | type '{}' takes no type arguments; remove the arguments af |
| 77 | resolve_type_with_kinds | new | E1016 | class_constraint_arity | type class '{}' constrains exactly one type, as in '{}<'a> |
| 78 | resolve_type_with_kinds | new | E1015 | type_constructor_not_applied | a type constructor must be fully applied in a value type |
| 79 | resolve_type_with_kinds | new | E1005 | unsized_slice_type_position | '[T..]' is only valid directly after 'ref mut'; write 'ref |
| 80 | resolve_type_with_kinds | new | E1004 | length_used_as_type | expected a type, found the length {length}; a length belon |
| 81 | resolve_dyn | new | E1004 | dyn_needs_type_class | {kind}; dyn needs a type class such as dyn Shapes.Shape |
| 82 | resolve_length | new | E1005 | invalid_length_constant | constant '{name}' cannot be a length; a length constant ha |
| 83 | resolve_length | new | E1004 | type_used_as_length | expected a length, found the type '{name}' |
| 84 | resolve_length | new | E1002 | unknown_length_constant | unknown constant '{name}'; a length is an integer literal, |
| 85 | resolve_length | new | E1004 | expected_length | expected a length, such as '3', an 'i64' constant, or a le |
| 86 | resolve_length | new | E1010 | fixed_array_too_long | fixed array length {length} exceeds {MAX_FIXED_ARRAY_LENGT |
| 87 | record_arity | br | E1024 | alias_argument_count | type '{name}' expects {expected} type argument(s), found { |
| 88 | record_arity | br | E1004 | type_argument_count | type '{name}' expects {expected} type argument(s), found { |
| 89 | TypeAliasExpansion::expand | new | E1017 | type_alias_expansion_limit | type alias expansion exceeds the compiler limit; simplify  |
| 90 | TypeAliasExpansion::alias | new | E1024 | cyclic_type_alias | cyclic type alias '{}'; remove the recursive alias referen |
| 91 | TypeAliasExpansion::substitute | new | E1017 | type_alias_expansion_limit | type alias expansion exceeds the compiler limit; simplify  |
| 92 | reject_expanded_field_constraints | new | E1016 | field_type_constraint | {parts} cannot constrain type parameters; put the constrai |
| 93 | size_error | new | E1010 | value_layout_too_large | value layout exceeds {MAX_VALUE_BYTES} bytes; use smaller  |
| 94 | Layouts::record | new | E1010 | infinite_recursive_layout | recursive value layout for '{}'; recursive heap types are  |
| 95 | Layouts::record | new | E1010 | layout_nesting_limit | record nesting exceeds {MAX_NESTING} |
| 96 | Layouts::union | new | E1010 | infinite_recursive_layout | recursive union layout for '{}'; recursive heap types are  |
| 97 | Layouts::union | new | E1010 | layout_nesting_limit | union nesting exceeds {MAX_NESTING} |
| 98 | validate_exclusive_field | new | E1013 | exclusive_borrow_field_shape | <<message>> |
| 99 | Validation::check | new | E1013 | exclusive_borrow_field_shape | <<message>> |
| 100 | Validation::check | new | E1013 | union_payload_exclusive_borrow | union payloads cannot store mutable references; {} would s |
| 101 | Validation::check | new | E1005 | collection_element_exclusive_borrow | array and list elements cannot contain mutable references; |
| 102 | Validation::check | new | E1005 | shared_container_exclusive_borrow | {} cannot hold a mutable reference; its owners only read t |
| 103 | Validation::check | new | E1013 | task_result_is_reference | task results must be owned values, not references |
| 104 | Checker::bind | warn | W1004 | shadowed_local | local '{}' shadows an earlier binding in the same scope |
| 105 | Checker::let_rec_group | new | E1015 | internal_rec_binding_not_lambda | internal compiler error: a 'let rec' binding is not a lamb |
| 106 | Checker::use_error | new | E1005 | use_requires_drop | 'use' needs a value whose type implements Drop; {} does no |
| 107 | Checker::value_expression | new | E1009 | float_literal_type | a floating-point literal needs a binary or decimal floatin |
| 108 | Checker::value_expression | new | E1015 | dyn_dispatch_result_type | a dyn dispatch needs the method's result type |
| 109 | Checker::value_expression | new | E1005 | range_as_value | a range is an enumerable expression for 'for...in', not a  |
| 110 | Checker::value_expression | new | E1023 | jump_outside_loop | {keyword} can only target an enclosing for or while loop i |
| 111 | Checker::value_expression | new | E1004 | empty_list_needs_annotation | an empty list needs a type annotation, for example '[\|i64 |
| 112 | Checker::value_expression | new | E1005 | new_on_fixed_array | 'new' creates a heap array or list; remove 'new' to create |
| 113 | Checker::value_expression | new | E1002 | unknown_module_member | <<message>> |
| 114 | Checker::value_expression | new | E1005 | index_non_indexable | indexing requires an array, list, string, or utf8string |
| 115 | Checker::value_expression | new | E1005 | deref_non_reference | dereference requires a reference |
| 116 | Checker::value_expression | new | E1014 | replace_exclusive_slice | cannot replace an exclusive slice as a whole because its l |
| 117 | Checker::value_expression | new | E1012 | assign_to_immutable_place | assignment replaces a mutable binding; record fields, arra |
| 118 | Checker::require_mutable_reference | new | E1014 | mutate_through_shared_reference | cannot mutate or exclusively reborrow through a shared ref |
| 119 | Checker::check_format_specs | new | E1003 | format_spec_type_unknown | format spec '{spelled}' needs a concrete numeric type, fou |
| 120 | Checker::check_format_specs | new | E1003 | format_spec_type_mismatch | format spec '{spelled}' does not apply to {}; {reason} |
| 121 | Checker::record_literal | new | E1007 | record_has_no_field | record '{}' has no field '{}' |
| 122 | Checker::record_literal | new | E1007 | missing_record_fields | missing fields: {} |
| 123 | Checker::slice | new | E1005 | slice_of_fixed_array | a fixed-length array has no exclusive slices; replace the  |
| 124 | Checker::slice | new | E1005 | slice_non_array | a slice requires an array; lists and strings do not suppor |
| 125 | Checker::named_slice_source | new | E1014 | exclusive_slice_source | an exclusive slice must borrow a named array; bind the val |
| 126 | Checker::fixed_slice_source | new | E1005 | fixed_slice_source | slicing a fixed-length array requires a named value; bind  |
| 127 | Checker::fixed_array_literal | new | E1003 | fixed_array_literal_length | expected {} elements for '{}', found {found} |
| 128 | Checker::whole_array_error | new | E1005 | exclusive_slice_whole_array_parameter | an exclusive slice cannot be passed as 'ref mut [T]' becau |
| 129 | Checker::coerce_slice_argument | new | E1014 | mutate_through_shared_reference | cannot mutate or exclusively reborrow through a shared ref |
| 130 | Checker::record_update | new | E1005 | record_update_non_record | record update requires a record value |
| 131 | Checker::record_update | new | E1007 | record_has_no_field | record '{}' has no field '{}' |
| 132 | Checker::field_access | new | E1007 | unknown_field | unknown field '{}' |
| 133 | Checker::field_access | new | E1007 | field_access_non_record | field access requires a record, or '.length' on an array,  |
| 134 | Checker::record_storage | new | E1022 | opaque_type_representation | the representation of '{}' is opaque; use its module API |
| 135 | Checker::integer_literal | new | E1009 | integer_literal_type | an integer literal needs an integer type |
| 136 | Checker::integer_literal | new | E1009 | integer_literal_out_of_range | integer literal is outside the range of {} |
| 137 | Checker::qualified_function | new | E1002 | unknown_function | unknown function '{}'{hint} |
| 138 | Checker::name | new | E1002 | unknown_value | unknown value '{}' |
| 139 | Checker::dyn_of | new | E1015 | dyn_of_needs_expected_type | Dyn.of needs an expected dyn type here; annotate the bindi |

### `src/polymorph.rs`（102 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | growth_error | new | E1017 | polymorphic_recursion_growth | polymorphic recursion grows the types of '{name}' without  |
| 2 | bounded_type | new | E1017 | polymorphic_type_expansion_limit | polymorphic type expansion exceeds the compiler limit; sim |
| 3 | require_concrete | new | E1015 | concrete_type_required | a concrete type is required here; add a type annotation |
| 4 | Inference::unify | new | E1015 | constructor_kind_mismatch | type constructor application has an incompatible kind |
| 5 | Inference::unify | new | E1015 | infinite_inferred_type | infinite inferred type |
| 6 | Inference::unify | new | E1003 | type_mismatch | expected {}, found {} |
| 7 | Classes::collect | new | E1004 | std_item_missing | Parse needs the standard union Maybe<'a> = None \| Some of |
| 8 | Classes::collect | new | E1004 | std_item_missing | {class} needs the standard module Json |
| 9 | Classes::collect | new | E1004 | std_item_missing | Encode and Decode need the standard union Result<'a, 'e> |
| 10 | Classes::collect | new | E1016 | class_method_constraint | class methods use the class parameter; method-specific con |
| 11 | Classes::collect | new | E1016 | class_method_constraint | class methods cannot add constraints to the class paramete |
| 12 | Classes::collect | new | E1016 | class_method_type_variable | each class method must mention exactly the declared class  |
| 13 | Classes::collect | new | E1016 | class_without_methods | a class needs at least one method |
| 14 | Classes::collect | new | E1015 | superclass_kind_mismatch | superclass parameter kind does not match the class paramet |
| 15 | Classes::collect | new | E1027 | superclass_type_variable | superclasses may mention only the class type variable |
| 16 | Classes::constraint_kinds | new | E1015 | type_variable_kind_conflict | type variable is constrained at incompatible kinds |
| 17 | Classes::resolve_constraints | new | E1017 | class_constraint_limit | too many class constraints |
| 18 | Classes::resolve_constraints | new | E1027 | class_context_not_a_class | class contexts require type class constraints |
| 19 | Classes::resolve_constraints | man | E1027 | unknown_class_in_context | recode: error.code = "E1027" after resolving a class name  |
| 20 | Classes::validate_superclasses | new | E1027 | superclass_cycle | superclass constraints form a cycle |
| 21 | Classes::validate_superclasses | new | E1017 | superclass_resolution_limit | superclass resolution exceeds depth 64 |
| 22 | Classes::superclasses | new | E1017 | superclass_resolution_limit | superclass constraint expansion exceeds the compiler limit |
| 23 | Classes::dyn_error | new | E1028 | dyn_incompatible_class | {} is not allowed: {reason}; use a generic function with { |
| 24 | Classes::resolve | new | E1016 | unknown_type_class | unknown type class '{}' |
| 25 | Classes::instances | new | E1016 | default_body_unknown_method | a default body must name a declared class method |
| 26 | Classes::instances | new | E1015 | instance_head_not_constructor | higher-kinded instances require a named constructor head |
| 27 | Classes::instances | new | E1027 | instance_context_variable | instance context mentions a type variable absent from its  |
| 28 | Classes::instances | new | E1016 | builtin_instance_override | built-in instances and marker classes cannot be overridden |
| 29 | Classes::instances | new | E1017 | instance_overlap_limit | instance overlap checking exceeds 1024 pairs |
| 30 | Classes::instances | new | E1016 | overlapping_instance | overlapping instance for {}<{}> |
| 31 | Classes::instances | new | E1016 | instance_unknown_method | unknown method '{}' for {} |
| 32 | Classes::instances | new | E1016 | instance_missing_method | missing method '{}.{}' |
| 33 | Classes::check_superclasses | new | E1027 | instance_missing_superclass | instance context does not entail its superclass; add the r |
| 34 | Classes::check_superclasses | man | E1025 | derived_instance_invalid | recode when instance.derived (code E1025) |
| 35 | Classes::check_superclasses | man | E1027 | instance_check_failed | recode when not derived (code E1027) |
| 36 | Classes::derived_error | man | E1025 | derived_instance_invalid | recode of E1005/E1027 raised inside a derived method (code |
| 37 | Classes::normalize | new | E1025 | default_derive_recursive | derived Default follows a recursive first case; implement  |
| 38 | Classes::normalize | new | E1017 | instance_resolution_cycle | instance constraint resolution forms a cycle |
| 39 | Classes::normalize | new | E1017 | instance_resolution_limit | instance constraint resolution exceeds depth 64 or 128 obl |
| 40 | Classes::method | new | E1002 | class_has_no_method | type class '{class_name}' has no method '{}' |
| 41 | Classes::method | new | E1016 | explicit_drop_call | 'Drop.drop' runs automatically when a value is dropped; le |
| 42 | Classes::validate_instance | new | E1005 | capture_rc_in_function_value | cannot capture {} in a function value; function values may |
| 43 | Classes::validate_instance | new | E1013 | task_requires_send | tasks require Send values; {} holds an Rc or Rc.Weak, whos |
| 44 | Classes::validate_instance | new | E1005 | capture_unsendable_in_function_value | cannot capture {ty} in a function value; function values m |
| 45 | Classes::validate_instance | new | E1013 | task_requires_send | tasks require Send values; {reason}; give the value to one |
| 46 | Classes::validate_instance | new | E1005 | capture_sync_primitive_in_function_value | cannot capture {} in a function value; a function value ma |
| 47 | Classes::validate_instance | new | E1013 | task_requires_sync | tasks can share only Sync values; {} is not Sync (an Rc, a |
| 48 | Classes::validate_instance | new | E1005 | atomic_element_type | atomic values must be i8, i16, i32, i64, i8u, i16u, i32u,  |
| 49 | Classes::validate_instance | new | E1005 | capture_exclusive_in_reusable_function | cannot capture {} in a reusable function; fully apply excl |
| 50 | Classes::validate_instance | new | E1013 | task_requires_owned_value | tasks require owned values; {} contains a reference |
| 51 | Classes::validate_instance | new | E1005 | no_instance | no instance for {}<{}>; define an instance or use a suppor |
| 52 | validate_drop_instance | new | E1016 | drop_instance_target | only records and unions declared in this program can imple |
| 53 | validate_drop_instance | new | E1016 | drop_instance_generic | a Drop instance must cover every instantiation; write ever |
| 54 | validate_drop_instance | new | E1016 | drop_instance_constraint | Drop instances cannot have constraints; drop must work for |
| 55 | validate_format_instance | new | E1016 | format_instance_target | only records and unions declared in this program can imple |
| 56 | instance_function | new | E1006 | instance_method_parameter_count | method '{}' expects {} parameters |
| 57 | Checker::builtin | new | E1022 | private_std_primitive | IO primitives are private to the standard IO module; compo |
| 58 | Checker::builtin | new | E1022 | private_std_primitive | operating-system primitives are private to the standard Fi |
| 59 | Checker::builtin | new | E1022 | private_std_primitive | network primitives are private to the standard Net module; |
| 60 | Checker::builtin | new | E1022 | private_std_primitive | Debug.__print_string is private to the standard Debug modu |
| 61 | Checker::builtin | new | E1022 | private_std_primitive | the property seed is private to the standard Gen module; s |
| 62 | Checker::builtin | new | E1022 | private_std_primitive | the GPU runtime primitives are private to the standard Gpu |
| 63 | Checker::builtin | new | E1022 | private_std_primitive | the continuation primitive is private to the standard Asyn |
| 64 | Checker::builtin | new | E1022 | private_std_primitive | the arena id primitive is private to the standard Arena mo |
| 65 | Checker::builtin | new | E1022 | private_std_primitive | the channel close primitive is private to the standard Cha |
| 66 | Checker::builtin | new | E1022 | private_std_primitive | Unicode tables are private to the standard Unicode and Reg |
| 67 | Checker::builtin | new | E1015 | dyn_of_arity | Dyn.of must be applied to exactly one argument where a dyn |
| 68 | Checker::builtin | new | E1005 | std_representation_mismatch | Seq.next requires the standard Seq record |
| 69 | Checker::builtin | new | E1005 | std_representation_mismatch | Seq.next requires the standard head and step representatio |
| 70 | Checker::builtin | new | E1005 | std_representation_mismatch | Seq.next requires the standard Maybe union |
| 71 | Checker::builtin | new | E1005 | std_representation_mismatch | Seq.next requires Maybe.None and Maybe.Some in standard or |
| 72 | Checker::builtin | new | E1005 | std_representation_mismatch | Async.__resume requires the standard Async.Next record |
| 73 | Checker::builtin | new | E1005 | std_representation_mismatch | {} requires the standard Owned.Function record |
| 74 | Checker::solve_families | new | E1015 | fixed_array_length_unknown | cannot determine the length of 'FixedArray.init'; annotate |
| 75 | Checker::solve_families | new | E1005 | fixed_array_init_annotation | 'FixedArray.init' creates a fixed-length array; annotate a |
| 76 | Checker::solve_families | br | E1015 | simd_vector_type_unknown | this SIMD operation needs a concrete vector type (input ty |
| 77 | Checker::solve_families | br | E1005 | simd_operation_vector_type | this SIMD operation needs a concrete vector type (input ty |
| 78 | Checker::solve_families | new | E1005 | simd_lane_count_mismatch | of_lanes count does not match the vector type |
| 79 | Checker::solve_families | new | E1005 | no_wider_integer | 128-bit integers have no wider integer type |
| 80 | Checker::solve_families | new | E1015 | concrete_integer_required | this builtin needs a concrete integer argument type at its |
| 81 | Checker::type_function | new | E1015 | type_variable_absent_from_signature | type variable '{}' is absent from this function's signatur |
| 82 | Checker::type_function | new | E1016 | missing_explicit_constraint | '{}.{} requires an explicit @'{} : #{} constraint |
| 83 | Checker::annotation | new | E1015 | type_variable_absent_from_signature | type annotation mentions a variable absent from this funct |
| 84 | Checker::call_signature | new | E1005 | call_non_function | only a function value can be called |
| 85 | Checker::call_signature | new | E1006 | too_many_arguments_applied | cannot apply {count} arguments to a function accepting {}  |
| 86 | Checker::call_signature | new | E1005 | call_non_function | only a function value can be called |
| 87 | Checker::finish | new | E1015 | ref_operand_inferred_late | 'ref' must know whether its operand is a reference when it |
| 88 | Checker::finish | new | E1015 | ambiguous_polymorphic_use | ambiguous polymorphic use; add a concrete type annotation  |
| 89 | Checker::finish | new | E1003 | type_mismatch | expected an integer, found {} |
| 90 | solve_members | new | E1017 | function_constraint_limit | too many distinct function constraints in one function |
| 91 | solve_members | new | E1017 | function_constraint_limit | too many distinct function constraints; polymorphic recurs |
| 92 | captures | new | E1005 | rec_function_captures_generic | a 'let rec' function cannot capture '{}', whose type {} ha |
| 93 | specialize | new | E1017 | polymorphic_constraint_limit | too many distinct polymorphic constraints in one function |
| 94 | specialize | new | E1017 | polymorphic_constraint_limit | constraint expansion exceeds the compiler limit; polymorph |
| 95 | specialize | new | E1027 | undeclared_default_constraint | default method requires an undeclared constraint; add it a |
| 96 | specialize | new | E1027 | undeclared_instance_constraint | instance method requires an undeclared constraint; add it  |
| 97 | specialize | new | E1015 | polymorphic_entry_point | the application entry point cannot be polymorphic |
| 98 | Specializer::request | new | E1017 | specialization_limit | more than {} specializations of generic functions; {advice |
| 99 | Specializer::dyn_of | new | E1013 | dyn_holds_borrow_without_region | a dyn value without a region cannot hold borrowed data, an |
| 100 | Specializer::dyn_of | new | E1013 | dyn_holds_exclusive_borrow | a dyn value cannot hold exclusive borrows, and {} holds on |
| 101 | Specializer::dyn_of | new | E1005 | no_instance | no instance for {}<{}>; define an instance or use a suppor |
| 102 | Specializer::operator_call | new | E1005 | no_instance_for_operator | no instance for this operator |

### `src/control.rs`（20 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | combine | new | E1017 | pattern_alternatives_limit | pattern alternatives exceed the compiler limit; split the  |
| 2 | Checker::control_expression | new | E1005 | for_over_fixed_array | a fixed-length array is not enumerable; iterate 'for i in  |
| 3 | Checker::control_expression | new | E1005 | for_source_not_iterable | for...in expects an array, list, string, utf8string, integ |
| 4 | Checker::sequence_loop | new | E1017 | sequence_loop_depth_limit | sequence loop expansion exceeds the compiler depth limit;  |
| 5 | Checker::match_value | new | E1020 | or_pattern_binding_mismatch | both sides of an OR pattern must bind the same names |
| 6 | Checker::match_value | new | E1020 | or_pattern_binding_mismatch | both sides of an OR pattern must bind the same names |
| 7 | Checker::pattern_alternatives | new | E1020 | qualified_pattern_binding | a pattern binding cannot be a qualified name |
| 8 | Checker::pattern_alternatives | new | E1020 | expression_in_pattern | expressions are only allowed as recognizer arguments; use  |
| 9 | Checker::pattern_alternatives | new | E1017 | or_pattern_alternatives_limit | too many OR pattern alternatives |
| 10 | Checker::pattern_alternatives | new | E1020 | record_pattern_unknown_type | an unqualified record pattern needs a known record type |
| 11 | Checker::pattern_alternatives | new | E1007 | record_has_no_field | record has no field '{}' |
| 12 | Checker::pattern_alternatives | new | E1020 | array_pattern_on_fixed_array | fixed-length arrays cannot be destructured by patterns; in |
| 13 | Checker::pattern_case | new | E1004 | ambiguous_pattern_name | '{}' names both the union case '{}' and an active pattern  |
| 14 | Checker::case_pattern | new | E1020 | case_pattern_missing_payload | union case '{}' carries a payload; match it with a payload |
| 15 | Checker::case_pattern | new | E1020 | case_pattern_unexpected_payload | union case '{}' has no payload; remove the patterns after  |
| 16 | Checker::case_pattern | new | E1020 | case_pattern_payload_shape | union case '{}' carries one payload; match several values  |
| 17 | Checker::active_argument | new | E1020 | active_argument_not_value | a recognizer argument must be a value expression, not a bi |
| 18 | Checker::active_pattern | new | E1020 | unknown_case_or_active_pattern | unknown union case or active pattern '{}' |
| 19 | Checker::active_pattern | new | E1006 | active_pattern_argument_count | recognizer '{}' expects {extras} arguments{} |
| 20 | Checker::active_pattern | new | E1014 | active_pattern_mutates_input | active recognizers cannot mutate the matched input |

### `src/ownership.rs`（41 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | Checker::call_contract | own | E1013 | callback_contract_unsatisfied | <<message>> |
| 2 | Checker::check_result | own | E1013 | task_result_retains_borrow | task results cannot retain borrowed values |
| 3 | Checker::check_result | own | E1013 | result_region_mismatch | returned borrow does not match the declared result region; |
| 4 | Checker::access | own | E1012 | use_after_move | use of moved or partially moved value '{name}' |
| 5 | Checker::access | own | E1012 | drop_replace_whole_value | cannot replace the whole value inside Drop.drop; read or b |
| 6 | Checker::access | own | E1012 | drop_pass_exclusive_value | cannot pass the value on as 'ref mut' inside Drop.drop; re |
| 7 | Checker::access | own | E1014 | mutate_immutable_value | mutable access requires 'let mut' or an exclusive referenc |
| 8 | Checker::access | own | E1014 | borrow_conflict | access conflicts with a live borrow; use the reference or  |
| 9 | Checker::place | own | E1013 | reference_without_owner | reference has no live owner |
| 10 | Checker::place | own | E1013 | borrow_of_temporary | borrow requires a local place; bind the temporary with 'le |
| 11 | Checker::shared_exclusive | own | E1014 | mutate_through_shared_reference | cannot mutate or exclusively reborrow through a shared ref |
| 12 | Checker::store_through | own | E1013 | store_region_unnamed | {UNNAMED}; name the target's region apart from the referen |
| 13 | Checker::store_through | own | E1013 | store_local_borrow_through_reference | cannot store a borrow of a local value through a reference |
| 14 | Checker::store_through | own | E1013 | store_region_mismatch | the stored borrow does not have the region of the referenc |
| 15 | Checker::store_through | own | E1013 | store_region_unnamed | <<UNNAMED>> |
| 16 | Checker::store_arguments | own | E1013 | self_referential_store | cannot store a reference inside its own owner |
| 17 | Checker::read_places | own | E1012 | move_out_of_collection_element | cannot move a non-Copy element out of an array or list; bo |
| 18 | Checker::read_places | own | E1012 | move_out_of_collection_element | cannot move a non-Copy value out of an array or list eleme |
| 19 | Checker::read_places | own | E1012 | move_out_of_reference_binding | cannot move a non-Copy pattern variable bound through a re |
| 20 | Checker::read_places | own | E1012 | move_out_of_drop_type | <<MOVE_OUT_OF_DROP>> |
| 21 | Checker::read_places | own | E1013 | nested_borrow_unsupported | nested borrowed values require explicit lifetime parameter |
| 22 | Checker::read_places | own | E1014 | move_reborrowed_exclusive_reference | cannot move an exclusive reference while it is reborrowed |
| 23 | Checker::eval | own | E1013 | borrow_across_async_boundary | <<ASYNC_BORROW>> |
| 24 | Checker::eval_block | own | E1013 | borrow_outlives_block | borrowed value does not live long enough to leave this blo |
| 25 | Checker::eval_block | own | E1013 | assignment_extends_borrow | assignment lets a reference outlive its owner |
| 26 | Checker::eval_call | own | E1013 | mutex_result_retains_borrow | Mutex.with_lock results must be proven free of borrowed en |
| 27 | Checker::eval_call | own | E1013 | dyn_holds_borrow_without_region | a dyn value without a region cannot hold borrowed data; pa |
| 28 | Checker::eval_call | own | E1013 | channel_item_holds_borrow | a channel item cannot hold a borrow, including a borrowed  |
| 29 | Checker::capture_loans | own | E1013 | task_capture_retains_borrow | task captures cannot retain borrowed values, including bor |
| 30 | Checker::eval_value | own | E1013 | must_apply_directly | parallel operations must be fully applied directly; their  |
| 31 | Checker::eval_value | own | E1013 | must_apply_directly | Mutex.with_lock must be fully applied directly when its re |
| 32 | Checker::eval_value | own | E1013 | must_apply_directly | Channel.send must be fully applied directly when its item  |
| 33 | Checker::eval_value | own | E1013 | self_referential_store | cannot store a reference inside its own owner |
| 34 | Checker::eval_value | own | E1013 | task_capture_retains_borrow | task captures cannot retain borrowed values, including bor |
| 35 | Checker::eval_value | own | E1013 | task_scope_shares_borrowed_environment | task scopes can share only values with proven owned enviro |
| 36 | Checker::eval_value | own | E1013 | parallel_input_borrows_environment | parallel input elements must have proven owned environment |
| 37 | Checker::eval_value | own | E1013 | parallel_callback_borrows_environment | parallel callbacks and values cannot retain borrowed envir |
| 38 | Checker::eval_value | own | E1013 | parallel_result_retains_borrow | parallel callback results must be proven free of borrowed  |
| 39 | Checker::eval_value | own | E1012 | update_drop_type_value | cannot update a value whose type implements Drop; construc |
| 40 | Checker::eval_value | own | E1012 | move_out_of_collection_element | cannot move a non-Copy array or list element; borrow it in |
| 41 | Checker::eval_value | own | E1012 | move_out_of_drop_type | <<MOVE_OUT_OF_DROP>> |

### `src/ownership_control.rs`（4 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | Checker::raise_edge | own | E1017 | too_many_raise_points | too many raise points in one try; split the try body |
| 2 | Checker::eval_loop_jump | own | E1017 | too_many_loop_exits | too many loop exit paths; split the loop body |
| 3 | Checker::finish_control_scope | own | E1013 | borrow_outlives_binding | a borrowed value cannot outlive its pattern or iteration b |
| 4 | Checker::check_loop | own | E1017 | loop_ownership_limit | loop ownership analysis exceeds the compiler limit; simpli |

### `src/exhaustiveness.rs`（4 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | Checker::check_coverage | warn | W1003 | unreachable_match_arm | unreachable match arm; previous patterns already cover thi |
| 2 | Checker::check_coverage | new | E1021 | non_exhaustive_match | match is not exhaustive; missing: {} |
| 3 | Search::limit | new | E1017 | pattern_alternatives_limit | pattern alternatives exceed the compiler limit; split the  |
| 4 | Search::charge | new | E1017 | exhaustiveness_search_limit | the match is too large to check for exhaustiveness; split  |

### `src/exceptions.rs`（3 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | Checker::try_parts | new | E1004 | std_item_missing | try needs the standard library function Exception.of_code |
| 2 | Checker::bigint_literal | new | E1009 | invalid_bigint_digit | invalid digit in a bigint literal |
| 3 | Checker::bigint_literal | new | E1004 | std_item_missing | bigint literals need the standard library function BigInt. |

### `src/derive.rs`（5 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | instances | new | E1017 | deriving_instance_limit | deriving exceeds 1024 instances |
| 2 | json_names | new | E1025 | json_name_without_deriving | '@json' names a {place} only for deriving (Encode, Decode) |
| 3 | json_names | new | E1025 | duplicate_json_name | the JSON name "{}" of {place} '{}' repeats another; give e |
| 4 | Build::make | new | E1017 | deriving_expression_limit | deriving exceeds the expression depth or node limit |
| 5 | Build::operators | new | E1025 | deriving_unavailable | this deriving implementation is not available yet |

### `src/higher_kinds.rs`（6 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | arity | new | E1015 | higher_order_kind_argument | phase-1 constructors accept value types, not higher-order  |
| 2 | resolve_constructor | new | E1015 | expected_type_constructor | expected a named type constructor with the declared kind |
| 3 | resolve_constructor | new | E1015 | scalar_as_type_constructor | a scalar value type is not a type constructor |
| 4 | resolve_constructor | new | E1015 | higher_kinded_alias | higher-kinded type aliases are not supported; use the unde |
| 5 | resolve_constructor | new | E1015 | extern_type_as_constructor | an extern type takes no type arguments, so it is not a typ |
| 6 | resolve_constructor | new | E1015 | constructor_kind_mismatch | type constructor kind does not match the class parameter |

### `src/recursive.rs`（4 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | Graph::visit | new | E1010 | infinite_recursive_layout | recursive value layout must pass through a union with a fi |
| 2 | Graph::visit | new | E1017 | recursive_type_expansion_limit | recursive type expansion exceeds the compiler limit |
| 3 | changes_arguments | new | E1017 | recursive_type_changes_arguments | recursive generic type changes its arguments; use a non-gr |
| 4 | analyze | new | E1010 | recursive_union_without_base_case | recursive union has no finite value; add a base case or an |

### `src/recursion.rs`（1 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | check_graph | new | E1019 | missing_rec_marker | recursive function '{}' requires 'rec' on its declaration  |

### `src/closures.rs`（3 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | Checker::lambda | new | E1006 | owned_function_parameter_count | Owned.function takes a lambda with one parameter; take a t |
| 2 | Checker::group_lambda | new | E1013 | owned_function_captures_reference | an owned function captures only owned values; {} contains  |
| 3 | lower_expression | new | E1012 | move_out_of_owned_function | cannot move '{}' out of an owned function; borrow it with  |

### `src/closures_rec.rs`（3 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | group_len | new | E1015 | internal_rec_group_unbound | internal compiler error: the members of a 'let rec' group  |
| 2 | lower | new | E1015 | internal_rec_not_bound_by_block | internal compiler error: a 'let rec' function is not bound |
| 3 | lower_group | new | E1012 | move_out_of_rec_function | cannot move '{}' out of a 'let rec' function, which borrow |

### `src/abi.rs`（4 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | link_name_error | new | E1008 | invalid_extern_symbol | invalid extern symbol '{symbol}'; use a C identifier of at |
| 2 | link_name_error | new | E1008 | reserved_extern_symbol | extern symbol '{symbol}' is reserved by Tsuzuri; choose a  |
| 3 | link_name_error | new | E1008 | reserved_extern_symbol | extern symbol '{symbol}' is reserved by Tsuzuri because th |
| 4 | link_name_error | new | E1008 | invalid_wasm_import_module | invalid WASM import module '{module}'; use 1 to 255 ASCII  |

### `src/copies.rs`（1 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | warnings | warn | W1006 | implicit_copy_of_large_value | <<message(&site.ty)>> |

### `src/warnings.rs`（4 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | unused_locals | warn | W1001 | unused_local | unused local '{}'; prefix it with '_' to silence this warn |
| 2 | unused_private | warn | W1002 | unused_private_function | unused private function '{}'; remove it or make it public |
| 3 | unused_private | warn | W1002 | unused_private_type | unused private type '{}'; remove it or make it public |
| 4 | deprecated_uses | warn | W1005 | deprecated_use | <<used.message>> |

### `src/lib.rs`（2 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | parse_input | new | E1011 | invalid_std_path | invalid standard library path '{}'; std sources are flat f |
| 2 | module_name_from_relative | new | E1011 | invalid_source_path | source paths must be relative ASCII module segments withou |

### `src/gpu.rs`（27 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | validate_calls | uns | E1018 | gpu_operation_not_direct | GPU operations cannot escape as function values; use direc |
| 2 | validate_calls | uns | E1018 | gpu_operation_not_direct | GPU operations require direct full application |
| 3 | validate_calls | uns | E1018 | gpu_buffer_element_type | GPU operations require a concrete buffer result |
| 4 | validate_calls | uns | E1018 | gpu_buffer_element_type | GPU buffers require 32/64-bit integer or binary floating-p |
| 5 | validate_calls | uns | E1018 | gpu_callback_not_known | GPU init/map operations require a known function or captur |
| 6 | extract_kernel | uns | E1018 | gpu_kernel_missing | GPU kernel function does not exist |
| 7 | extract_kernel | uns | E1018 | gpu_kernel_signature | GPU kernels require one scalar parameter and a scalar resu |
| 8 | extract_kernel | uns | E1018 | gpu_kernel_recursion | GPU kernels cannot call recursive functions |
| 9 | extract_kernel | new | E1017 | gpu_kernel_limit | GPU kernel exceeds 128 call levels or 1024 functions |
| 10 | extract_kernel | uns | E1018 | gpu_kernel_signature | GPU kernel calls must have scalar signatures and no captur |
| 11 | extract_kernel | new | E1017 | gpu_kernel_limit | GPU kernel exceeds 65536 expression nodes |
| 12 | extract_kernel | uns | E1018 | gpu_unsupported_type | GPU kernels cannot allocate or carry nonscalar values |
| 13 | extract_kernel | uns | E1018 | gpu_callback_not_known | GPU kernels require direct known calls without captures |
| 14 | extract_kernel | uns | E1018 | gpu_operation_not_direct | GPU kernel calls must be fully applied |
| 15 | extract_kernel | uns | E1018 | gpu_unsupported_construct | GPU kernels do not support allocation, borrows, host calls |
| 16 | GpuKernel::emit_wgsl | uns | E1018 | gpu_unsupported_type | strict WebGPU kernels require i32 or i32u buffer lanes; 64 |
| 17 | GpuKernel::emit_wgsl | uns | E1018 | gpu_unsupported_type | <<RELAXED_BOOL_LANE>> |
| 18 | wgsl_type | uns | E1018 | gpu_unsupported_type | <<RELAXED_TYPES>> |
| 19 | wgsl_type | uns | E1018 | gpu_unsupported_type | WGSL cannot preserve this scalar type; use the explicit CP |
| 20 | too_deep | new | E1017 | gpu_kernel_limit | GPU kernel exceeds {MAX_STATEMENT_DEPTH} levels of WGSL st |
| 21 | Wgsl::expression | uns | E1018 | gpu_unsupported_operator | WGSL kernels do not support '**'; multiply explicitly |
| 22 | Wgsl::expression | uns | E1018 | gpu_unsupported_operator | WGSL kernel division/remainder may trap on the CPU and are |
| 23 | Wgsl::expression | uns | E1018 | gpu_unsupported_operator | <<RELAXED_CASTS>> |
| 24 | Wgsl::expression | uns | E1018 | gpu_unsupported_operator | WGSL kernel casts require 32-bit integer operands |
| 25 | Wgsl::expression | uns | E1018 | gpu_unsupported_construct | GPU assignment requires a scalar local |
| 26 | Wgsl::expression | uns | E1018 | gpu_callback_not_known | GPU calls require known functions |
| 27 | Wgsl::expression | uns | E1018 | gpu_unsupported_construct | expression is not supported by relaxed WGSL generation |

### `src/gpu/spirv.rs`（27 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | Emitter::kind_of | uns | E1018 | gpu_unsupported_type | Vulkan kernels do not support f64; use the CPU reference p |
| 2 | Emitter::kind_of | uns | E1018 | gpu_unsupported_type | Vulkan kernels support bool, 32/64-bit integer, and f32 sc |
| 3 | Emitter::kind_of | uns | E1018 | gpu_unsupported_type | relaxed GPU kernels support f32, i32, and i32u values; 64- |
| 4 | Emitter::lane_of | uns | E1018 | gpu_unsupported_type | Vulkan kernels require i32, i32u, i64, i64u, or f32 buffer |
| 5 | Emitter::function | uns | E1018 | gpu_kernel_signature | GPU kernel functions must return a scalar value |
| 6 | Emitter::function | uns | E1018 | gpu_kernel_signature | GPU kernel function result type mismatch |
| 7 | Emitter::value | uns | E1018 | gpu_unsupported_construct | a GPU kernel expression has no value |
| 8 | Emitter::expression | uns | E1018 | gpu_unsupported_construct | an integer literal needs an integer type |
| 9 | Emitter::expression | uns | E1018 | gpu_unsupported_construct | a floating-point literal needs the f32 type |
| 10 | Emitter::expression | uns | E1018 | gpu_unsupported_construct | a GPU local needs a value |
| 11 | Emitter::expression | uns | E1018 | gpu_unsupported_construct | GPU assignment requires a scalar local |
| 12 | Emitter::expression | uns | E1018 | gpu_unsupported_construct | GPU assignment requires a mutable scalar local |
| 13 | Emitter::expression | uns | E1018 | gpu_callback_not_known | GPU calls require known functions |
| 14 | Emitter::expression | uns | E1018 | gpu_unsupported_construct | a GPU kernel calls a function that is not emitted |
| 15 | Emitter::expression | uns | E1018 | gpu_unsupported_construct | expression is not supported by SPIR-V generation |
| 16 | Emitter::local | uns | E1018 | gpu_unsupported_construct | a GPU kernel reads a local that is not in scope |
| 17 | Emitter::unary | uns | E1018 | gpu_unsupported_operator | unsupported unary operator in a GPU kernel |
| 18 | Emitter::binary | uns | E1018 | gpu_unsupported_operator | strict GPU kernels cannot reproduce floating-point divisio |
| 19 | Emitter::binary | uns | E1018 | gpu_unsupported_operator | GPU kernels do not support floating-point remainder; compu |
| 20 | Emitter::binary | uns | E1018 | gpu_unsupported_operator | unsupported floating-point operator in a GPU kernel |
| 21 | Emitter::binary | uns | E1018 | gpu_unsupported_operator | unsupported boolean operator in a GPU kernel |
| 22 | Emitter::binary | uns | E1018 | gpu_unsupported_operator | a GPU shift count must have the type of the value |
| 23 | Emitter::binary | uns | E1018 | gpu_unsupported_operator | GPU kernels do not support '**'; multiply explicitly |
| 24 | Emitter::binary | uns | E1018 | gpu_unsupported_operator | GPU kernel division/remainder may trap on the CPU and are  |
| 25 | Emitter::branch | uns | E1018 | gpu_unsupported_construct | a GPU conditional needs a value in both branches |
| 26 | Emitter::cast | uns | E1018 | gpu_unsupported_operator | relaxed GPU kernels cannot reproduce Tsuzuri float-to-inte |
| 27 | Emitter::cast | uns | E1018 | gpu_unsupported_operator | unsupported cast in a GPU kernel |

### `src/llvm.rs`（15 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | emit_with_export_style | new | E2000 | option_requires_build | dllexport requires a native target |
| 2 | emit_with_debug_info | new | E2000 | debug_info_without_source_map | debug information requires a source map |
| 3 | emit_native | new | E2000 | option_requires_build | CPU dispatch is native-only |
| 4 | emit_trap_return | new | E2000 | option_requires_build | --trap-mode return is native-only |
| 5 | emit_test_runner_with | new | E2000 | index_out_of_range | invalid test index |
| 6 | emit_bench_runner_with | new | E2000 | index_out_of_range | invalid bench index |
| 7 | emit_program_in | new | E2000 | option_conflict | external runtime objects cannot be combined with trap, deb |
| 8 | emit_program_in | new | E2000 | option_conflict | the host allocator cannot be combined with WASM threads |
| 9 | emit_program_in | new | E2000 | option_conflict | --trap-mode return keeps its own tracked allocator |
| 10 | emit_program_in | new | E2000 | option_requires_build | the small allocator is for native targets; WebAssembly kee |
| 11 | validate_lowering | new | E1015 | internal_unlowerable_ir | internal compiler error: erroneous or unsaturated typed IR |
| 12 | clock_error | new | E1018 | bench_clock_outside_bench | {RULE}; {ADVICE} |
| 13 | clock_error | new | E1018 | bench_clock_outside_bench | <<message>> |
| 14 | validate_main | new | E2004 | entry_point_missing | an executable requires top-level entry-point code or 'def  |
| 15 | validate_main | new | E2004 | entry_point_result_type | top-level entry-point code must end with IO<T>, a number,  |

### `src/llvm_traps.rs`（3 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | Sites::base | new | E1017 | too_many_trap_locations | too many trap locations |
| 2 | Sites::base | new | E1017 | too_many_trap_locations | too many trap locations |
| 3 | instrument | new | E2000 | trap_info_without_source_map | trap information requires a source map |

### `src/llvm_ranges.rs`（1 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | RangeFlags::from_environment | new | E2000 | invalid_range_flag_setting | {ENVIRONMENT_VARIABLE} must be off, proven or audit, not ' |

### `src/llvm_cpu.rs`（1 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | multiversion | new | E1005 | cpu_function_wide_vector_boundary | a '@cpu' function cannot pass a SIMD vector wider than 128 |

### `src/trap.rs`（1 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | TrapSite::source | new | E2000 | invalid_trap_source_map | trap information requires a valid source map |

### `src/parallel.rs`（1 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | thread_setting | new | E2000 | invalid_thread_setting | {THREADS_VARIABLE} must be an integer from 1 to {MAX_THREA |

### `src/docgen.rs`（1 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | render_project | new | E2003 | doc_filename_collision | documentation filenames collide, including the reserved in |

