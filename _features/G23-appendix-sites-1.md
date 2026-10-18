# G23 付録: 発行箇所の表 — 構文・名前・型・定数（字句、構文、マニフェスト、定数評価、領域、多相、検査）

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

### `src/lexer.rs`（29 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | tokenize | new | E0003 | source_too_large | source exceeds the {MAX_SOURCE_BYTES}-byte limit; split th |
| 2 | Lexer::tokens | new | E0001 | unterminated_interpolation_hole | unterminated interpolation hole; close it with '}' |
| 3 | Lexer::token | new | E0001 | interpolation_hole_spans_lines | an interpolation hole must stay on one line; bind the valu |
| 4 | Lexer::token | new | E0001 | comment_in_interpolation_hole | comments are not allowed inside an interpolation hole |
| 5 | Lexer::byte_suffix | clo | E0001 | byte_literal_not_ascii | a 'B' literal holds only ASCII characters (U+0000 to U+007 |
| 6 | Lexer::byte_suffix | clo | E0001 | byte_literal_not_ascii | a 'B' literal holds only ASCII characters (U+0000 to U+007 |
| 7 | Lexer::byte_suffix | clo | E0001 | byte_suffix_on_unsupported_literal | only 'c'B, "text"B, and u8"text"B take the 'B' suffix |
| 8 | Lexer::no_byte_suffix | new | E0001 | byte_suffix_on_interpolated_string | an interpolated string does not take the 'B' suffix |
| 9 | Lexer::char_or_type_variable | new | E0001 | invalid_type_variable_start | a type variable starts with an apostrophe and an ASCII let |
| 10 | Lexer::char_or_type_variable | new | E0001 | char_literal_not_one_unit | a char literal contains exactly one UTF-16 code unit |
| 11 | Lexer::character | new | E0001 | utf8char_not_one_scalar | a utf8char literal contains exactly one Unicode scalar |
| 12 | Lexer::comment | new | E0001 | unterminated_block_comment | unterminated block comment |
| 13 | Lexer::digits | new | E0001 | numeric_separator_misplaced | numeric separators must occur between digits |
| 14 | Lexer::digits | new | E0001 | expected_digit | expected a digit |
| 15 | Lexer::number | new | E0001 | bigint_suffix_with_fraction | the 'I' suffix makes a bigint, which has no fraction or ex |
| 16 | Lexer::number | new | E0001 | invalid_numeric_literal | invalid numeric literal or type suffix |
| 17 | Lexer::number | new | E0001 | float_suffix_on_hex_or_binary_literal | a hexadecimal or binary literal takes an integer suffix, n |
| 18 | Lexer::string | new | E0001 | unterminated_string_literal | unterminated string literal |
| 19 | Lexer::string_char | new | E0001 | invalid_string_escape | invalid string escape |
| 20 | Lexer::string_char | new | E0001 | raw_newline_in_string | use '\ ' for a newline inside a string |
| 21 | Lexer::interpolated_text | new | E0001 | single_closing_brace_in_interpolation | a single '}' in an interpolated string must be written '}} |
| 22 | Lexer::interpolated_text | new | E0001 | unterminated_string_literal | unterminated string literal |
| 23 | Lexer::format_spec | new | E0001 | unterminated_interpolation_hole | unterminated interpolation hole; close it with '}' |
| 24 | Lexer::unicode_escape | new | E0001 | invalid_unicode_scalar_escape | invalid Unicode scalar escape |
| 25 | Lexer::symbol | new | E0001 | unexpected_character | unexpected character {character:?}; identifiers use ASCII |
| 26 | parse_format_spec | br | E0001 | invalid_format_spec | invalid format spec '{text}'; expected [[fill]align][+][wi |
| 27 | parse_format_spec | br | E0001 | format_field_out_of_range | format width and precision are at most {MAX_FORMAT_FIELD}  |
| 28 | parse_format_spec | br | E0001 | format_precision_on_integer | precision does not apply to integer format '{type_char}';  |
| 29 | parse_format_spec | br | E0001 | format_type_needs_precision | format type '{type_char}' needs a precision such as '.2{ty |

### `src/parse_control.rs`（22 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | Parser::function_name | new | E1020 | active_pattern_case_name | an active pattern case starts with an uppercase letter |
| 2 | Parser::function_name | par | E0002 | too_many_active_pattern_cases | too many active pattern cases |
| 3 | Parser::function_name | new | E1020 | partial_active_pattern_case_count | partial active patterns have exactly one case |
| 4 | Parser::implicit_active_union | new | E1020 | multi_case_active_pattern_result_type | multi-case active patterns use the implicit result type 'T |
| 5 | Parser::interpolation_holes | new | E0002 | too_many_interpolation_holes | an interpolated string has at most {MAX_INTERPOLATION_HOLE |
| 6 | Parser::interpolation_holes | new | E0002 | empty_interpolation_hole | an interpolation hole needs an expression; write '{{' for  |
| 7 | Parser::interpolation_holes | par | E0002 | expected_interpolation_close | expected '}' to close the interpolation hole |
| 8 | Parser::body_expression | par | E0002 | expected_body_expression | expected a body expression |
| 9 | Parser::type_product | par | E0002 | too_many_tuple_elements | too many tuple elements |
| 10 | Parser::for_source | new | E0002 | for_loop_needs_identifier | a for...to loop requires an identifier |
| 11 | Parser::explicit_lambda | par | E0002 | too_many_lambda_parameters | too many lambda parameters |
| 12 | Parser::explicit_lambda | par | E0002 | lambda_without_parameters | a lambda requires at least one parameter; use '\() -> ...' |
| 13 | Parser::checked_expression | par | E0002 | expected_expression_after_attribute | expected an expression; '@checked' is the only attribute o |
| 14 | Parser::checked_expression | par | E0002 | misplaced_checked_attribute | '@checked' on its own line applies to the next statement o |
| 15 | Parser::guarded_definition | par | E0002 | expected_where_bindings | expected indented bindings after 'where' |
| 16 | Parser::guarded_definition | par | E0002 | recursive_where_binding | 'where' bindings cannot be recursive; define the function  |
| 17 | Parser::guarded_definition | par | E0002 | where_binding_indentation | where bindings must have the same indentation |
| 18 | Parser::match_arms | par | E0002 | match_without_arms | expected at least one '\| pattern -> expression' arm |
| 19 | Parser::make_pattern | new | E0002 | pattern_nesting_too_deep | pattern nesting exceeds the compiler limit |
| 20 | Parser::pattern_atom | par | E0002 | negative_pattern_needs_number | a negative pattern requires a numeric literal |
| 21 | Parser::pattern_atom | par | E0002 | expected_pattern | expected a pattern |
| 22 | Parser::named_pattern | new | E1020 | null_pattern | Tsuzuri has no null values; null patterns are not supporte |

### `src/parser.rs`（121 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | duplicate_active_case | new | E1020 | duplicate_active_pattern_case | duplicate active pattern case '{}' |
| 2 | Parser::expect | par | E0002 | expected_token | expected {description} |
| 3 | Parser::enter | par | E0002 | syntax_nesting_too_deep | syntax nesting exceeds {MAX_NESTING} |
| 4 | Parser::ident | par | E0002 | expected_identifier | expected an identifier |
| 5 | Parser::program_all | new | E0002 | misplaced_doc_comment | doc comments attach to API declarations such as 'def', not |
| 6 | Parser::program_all | new | E0002 | misplaced_doc_comment | doc comments attach to API declarations such as 'def', not |
| 7 | Parser::program_all | par | E0002 | cpu_attribute_on_const_def | '@cpu' cannot apply to a 'const def'; the compiler evaluat |
| 8 | Parser::program_all | par | E0002 | misplaced_cpu_attribute | '@cpu' applies to the 'def' signature of a top-level funct |
| 9 | Parser::program_all | par | E0002 | class_needs_one_type_parameter | a type class requires exactly one type parameter, as in cl |
| 10 | Parser::program_all | new | E0002 | misplaced_doc_comment | doc comments attach to a class method's 'def', not its imp |
| 11 | Parser::program_all | new | E1001 | duplicate_function_definition | duplicate function definition |
| 12 | Parser::program_all | new | E0002 | let_function_needs_lambda | a declared top-level 'let' function needs a lambda such as |
| 13 | Parser::program_all | new | E1008 | export_on_extern_declaration | extern declarations cannot be exported |
| 14 | Parser::program_all | par | E0002 | misplaced_doc_comment | doc comments attach to typed declarations, not to implemen |
| 15 | Parser::program_all | par | E0002 | malformed_multi_case_active_pattern | multi-case active patterns use 'def (\|First\|Second\|) :: |
| 16 | Parser::program_all | new | E1019 | and_without_recursive_group | 'and' requires a preceding recursive declaration or defini |
| 17 | Parser::program_all | new | E1001 | duplicate_function_signature | duplicate function signature |
| 18 | Parser::program_all | par | E0002 | signature_uses_def | use 'def name :: ...' for a signature; 'fn' introduces its |
| 19 | Parser::program_all | new | E1001 | duplicate_function_definition | duplicate function definition |
| 20 | Parser::program_all | par | E0002 | malformed_multi_case_active_pattern | multi-case active patterns require an inline lambda and th |
| 21 | Parser::program_all | par | E0002 | typed_and_needs_body | a typed 'and' declaration requires '= ...' |
| 22 | Parser::program_all | new | E1001 | duplicate_function_definition | duplicate function definition |
| 23 | Parser::program_all | par | E0002 | export_on_implementation | put 'export' on the function signature |
| 24 | Parser::program_all | par | E0002 | misplaced_namespace_declaration | a namespace declaration must be the first declaration of t |
| 25 | Parser::program_all | par | E0002 | misplaced_using_declaration | using declarations belong at the top of the file, after th |
| 26 | Parser::program_all | par | E0002 | module_declaration_unsupported | module declarations are not supported; each .tz, .tt, or . |
| 27 | Parser::program_all | new | E0002 | definition_without_signature | function definition needs a 'def name :: ...' signature{hi |
| 28 | Parser::program_all | new | E0002 | signature_without_definition | function signature has no definition |
| 29 | Parser::visibility | new | E1022 | private_on_unsupported_declaration | <<message>> |
| 30 | Parser::misplaced_const | new | E0002 | misplaced_const_modifier | put 'const' on the 'def' signature; its 'fn', 'and', or 'l |
| 31 | Parser::const_declaration | new | E0002 | const_export_order | write 'export const def' to export a const function |
| 32 | Parser::type_colon | par | E0002 | signature_uses_double_colon | use '::' between a 'def' name and its type, as in 'def nam |
| 33 | Parser::cpu_attribute | par | E0002 | expected_cpu_target | expected a CPU target such as "avx2" |
| 34 | Parser::cpu_attribute | new | E0002 | unknown_cpu_target | unknown CPU target '{name}'; use "sse4.2", "avx2", "avx512 |
| 35 | Parser::cpu_attribute | new | E0002 | duplicate_cpu_target | CPU target '{name}' is listed twice |
| 36 | Parser::alias_declaration | new | E0002 | alias_name_on_same_line | write the alias name on the same line as '@alias', as in ' |
| 37 | Parser::alias_declaration | par | E0002 | expected_builder_alias | '@alias' takes a builder alias such as 'async': an identif |
| 38 | Parser::alias_declaration | br | E0002 | builder_alias_name_case | a builder alias starts with a lowercase letter |
| 39 | Parser::alias_declaration | br | E0002 | builder_alias_is_contextual_keyword | '{}' is a contextual keyword and cannot be a builder alias |
| 40 | Parser::alias_declaration | br | E0002 | duplicate_builder_alias | builder alias '{}' is declared twice in this file |
| 41 | Parser::alias_declaration | par | E0002 | expected_line_break | expected a line break after the builder alias |
| 42 | Parser::union_declaration | par | E0002 | expected_union_case_name | expected a union case name |
| 43 | Parser::derives | new | E1025 | underivable_class | only Eq, Ord, Display, Hash, Default, Encode and Decode ca |
| 44 | Parser::derives | new | E1001 | duplicate_derived_class | duplicate derived class |
| 45 | Parser::private_export | new | E1022 | private_export | 'private export' is not allowed; remove 'private' or remov |
| 46 | Parser::region_list | new | E1013 | invalid_region_names | region names must be distinct lowercase identifiers |
| 47 | Parser::region_list | new | E1017 | too_many_region_parameters | too many region parameters |
| 48 | Parser::region_list | par | E0002 | empty_region_list | a region list must not be empty |
| 49 | Parser::header_declaration | par | E0002 | header_path_on_same_line | write the {keyword} path on the same line as '{keyword}' |
| 50 | Parser::header_declaration | par | E0002 | header_path_spacing | join {keyword} path segments with '::' and no spaces, as i |
| 51 | Parser::header_declaration | new | E1017 | header_path_too_long | {keyword} paths are limited to 16 segments |
| 52 | Parser::header_declaration | par | E0002 | expected_line_break | expected a line break after the {keyword} path |
| 53 | Parser::path_segments | new | E1017 | namespace_path_too_long | namespace paths are limited to 16 segments before the modu |
| 54 | Parser::qualified_path | new | E1017 | qualified_name_too_long | qualified names exceed the 18-segment compiler limit |
| 55 | Parser::kind_expression | par | E0002 | expected_kind | expected '*' or a parenthesized kind |
| 56 | Parser::type_parameters | par | E0002 | type_parameters_need_angle_brackets | enclose type parameters in '<...>', as in Name<'a, 'b> |
| 57 | Parser::type_parameter | new | E0002 | length_parameter_type | a length parameter has type 'i64', as in 'const N: i64' |
| 58 | Parser::angle_list | par | E0002 | angle_bracket_spacing | '<' must immediately follow the name |
| 59 | Parser::angle_list | par | E0002 | too_many_type_arguments | too many type parameters or arguments |
| 60 | Parser::single_type_argument | par | E0002 | expected_single_type_argument | expected exactly one type argument |
| 61 | Parser::type_close | par | E0002 | expected_type_close | expected '>' after type parameters or arguments |
| 62 | Parser::instance_method | new | E0002 | let_function_needs_lambda | a 'let' method implementation needs a lambda |
| 63 | Parser::signature | par | E0002 | constraint_line_indentation | constraint lines must be indented after the 'def' signatur |
| 64 | Parser::signature | par | E0002 | malformed_constraint_line | expected a type variable after '@' on the same line |
| 65 | Parser::signature | par | E0002 | malformed_constraint_line | expected ':' on the constraint line |
| 66 | Parser::signature | par | E0002 | malformed_constraint_line | expected a type class or '#function' on the constraint lin |
| 67 | Parser::signature | par | E0002 | malformed_constraint_line | expected a function name after '#' on the same line |
| 68 | Parser::define | new | E1019 | mismatched_recursive_group | 'def' and its implementation must have matching 'rec'/'and |
| 69 | Parser::define | new | E1006 | definition_parameter_count_mismatch | signature expects {} parameters, definition has {} |
| 70 | Parser::json_attribute | par | E0002 | misplaced_json_attribute | only '@json "name"' can come before a {place} |
| 71 | Parser::json_attribute | par | E0002 | invalid_json_attribute | expected the JSON name as a string literal after '@json',  |
| 72 | Parser::json_attribute | par | E0002 | duplicate_json_attribute | a {place} takes one '@json' attribute |
| 73 | Parser::type_expr | par | E0002 | too_many_function_parameters | too many function parameters |
| 74 | Parser::quantified_type | par | E0002 | misplaced_region_list | write one region list before a function type |
| 75 | Parser::close_bracket_type | par | E0002 | list_type_with_length | list types use '[\|T\|]', without a length; use '[T; N]' f |
| 76 | Parser::fixed_array_length | par | E0002 | invalid_fixed_array_length | a fixed array length must be an integer literal without a  |
| 77 | Parser::type_arguments | par | E0002 | angle_bracket_spacing | '<' must immediately follow the name |
| 78 | Parser::type_arguments | par | E0002 | too_many_type_arguments | too many type parameters or arguments |
| 79 | Parser::dyn_type | par | E0002 | expected_dyn_class | expected a type class name after dyn |
| 80 | Parser::dyn_type | par | E0002 | dyn_with_type_arguments | dyn takes a class name without type arguments; write dyn S |
| 81 | Parser::dyn_type | par | E0002 | too_many_dyn_classes | too many classes after dyn |
| 82 | Parser::reference_type | new | E1013 | reference_region_count | a reference has exactly one region |
| 83 | Parser::make | new | E0002 | expression_nesting_too_deep | expression depth exceeds {MAX_NESTING}; introduce let bind |
| 84 | Parser::block_after_open | par | E0002 | misplaced_checked_attribute | @checked applies to a let or expression statement in a blo |
| 85 | Parser::task_return | par | E0002 | return_outside_task_end | 'return' is only allowed at the end of a task block |
| 86 | Parser::task_do | par | E0002 | do_bang_outside_task | 'do!' is only allowed inside a task block |
| 87 | Parser::binding | par | E0002 | use_bang_outside_computation | 'use!' is only allowed inside a task block or a computatio |
| 88 | Parser::binding | rec | E0002 | recursive_let_bang | <<LET_BANG_REC>> |
| 89 | Parser::binding | rec | E0002 | checked_let_rec | <<CHECKED_LET_REC>> |
| 90 | Parser::binding_rest | rec | E0002 | mutable_let_rec | <<REC_MUTABLE>> |
| 91 | Parser::binding_rest | rec | E0002 | let_rec_needs_lambda | <<REC_NEEDS_LAMBDA>> |
| 92 | Parser::recursive_member | rec | E0002 | checked_let_rec | <<CHECKED_LET_REC>> |
| 93 | Parser::binding_end | par | E0002 | expected_binding_end | expected ';' or a newline after the binding |
| 94 | Parser::computation_body | par | E0002 | expected_computation_body | expected a computation body |
| 95 | Parser::computation_sequence | par | E0002 | return_not_last_in_computation | 'return' and 'return!' must end a computation block |
| 96 | Parser::checked_statement | rec | E0002 | checked_let_rec | <<CHECKED_LET_REC>> |
| 97 | Parser::checked_statement | new | E0002 | misplaced_checked_attribute | @checked applies to a let, let!, do!, return, or expressio |
| 98 | Parser::recursive_statement | rec | E0002 | stray_and_member | <<STRAY_MEMBER>> |
| 99 | Parser::computation_statement | par | E0002 | use_bang_in_and_group | 'use!' cannot start an and! group; bind the sources with l |
| 100 | Parser::computation_binding | rec | E0002 | recursive_let_bang | <<LET_BANG_REC>> |
| 101 | Parser::computation_binding | rec | E0002 | recursive_let_bang | <<LET_BANG_REC>> |
| 102 | Parser::computation_binding | new | E1001 | duplicate_and_binding | duplicate and! binding name |
| 103 | Parser::computation_binding | new | E1017 | and_group_too_large | and! group exceeds the compiler binding limit |
| 104 | Parser::computation_match | par | E0002 | computation_match_without_arms | match! requires at least one '\| pattern -> computation' a |
| 105 | Parser::index_or_slice | new | E0002 | slice_must_be_borrowed | array slices must be borrows; write 'ref xs[start..end]' f |
| 106 | Parser::index_or_slice | new | E0002 | slice_without_bounds | a slice needs at least one bound; write 'ref mut xs[0..]'  |
| 107 | Parser::new_collection | par | E0002 | malformed_new_collection | expected an array or list after 'new': 'new [T](length, in |
| 108 | Parser::new_collection | new | E0002 | malformed_new_collection | expected '(' after the collection type; use 'new [{}](leng |
| 109 | Parser::primary | par | E0002 | mut_as_expression | 'mut' is not an expression; write 'ref mut x' to borrow ex |
| 110 | Parser::primary | par | E0002 | expected_expression | expected an expression |
| 111 | Parser::extern_declaration | new | E0002 | extern_type_with_link_name | extern type declarations take no link name; remove the str |
| 112 | Parser::extern_declaration | new | E0002 | extern_type_with_body | extern type declarations have no type parameters or defini |
| 113 | Parser::extern_declaration | new | E1008 | export_on_extern_declaration | extern declarations cannot be exported |
| 114 | Parser::extern_declaration | new | E1001 | duplicate_extern_declaration | duplicate extern or function declaration |
| 115 | Parser::link_name | new | E0002 | too_many_extern_link_names | extern takes at most two link name strings; write extern " |
| 116 | Parser::link_name | new | E0002 | invalid_link_name_text | extern link names must contain valid Unicode scalars |
| 117 | Parser::named_declaration | new | E0002 | invalid_link_name_text | {kind} names must contain valid Unicode scalars |
| 118 | Parser::named_declaration | new | E0002 | expected_link_name_string | expected a string literal {kind} name |
| 119 | Parser::identifier_expression | new | E1017 | qualified_name_too_long | qualified names exceed the 18-segment compiler limit |
| 120 | Parser::keyword_prefix | par | E0002 | prefix_keyword_operand | '{keyword}' takes exactly one operand; write '({form})' wh |
| 121 | integer | new | E0002 | integer_literal_too_large | integer literal is too large |

### `src/package.rs`（48 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | parse_lock | new | E1017 | lockfile_too_large | Tsuzuri.lock exceeds 1 MiB |
| 2 | parse_lock | new | E2007 | invalid_lockfile | Tsuzuri.lock is not a valid lockfile ({detail}); restore i |
| 3 | namespace | new | E1011 | invalid_package_name | package names must be lowercase ASCII kebab-case (at most  |
| 4 | create_project | br | E2000 | invalid_namespace_argument | invalid namespace '{namespace}'; use identifiers joined by |
| 5 | create_project | br | E2000 | cannot_derive_namespace | cannot derive a namespace from the folder name '{folder}'; |
| 6 | create_project | br | E2000 | new_project_io_failure | cannot {action} '{}': {cause} |
| 7 | create_project | br | E2000 | new_project_folder_not_empty | '{}' is not empty; choose a new or empty folder |
| 8 | parse_manifest | new | E1017 | manifest_too_large | package manifest exceeds 1 MiB |
| 9 | parse_manifest | lin | E0002 | manifest_section_order | expected [package] followed by optional [dependencies], [w |
| 10 | parse_manifest | lin | E0002 | unknown_manifest_key | unknown package key; expected name, version, namespace, or |
| 11 | parse_manifest | lin | E0002 | invalid_package_field | package fields must be nonempty and occur exactly once |
| 12 | parse_manifest | lin | E0002 | invalid_git_dependency | <<GIT_FORM>> |
| 13 | parse_manifest | lin | E0002 | invalid_dependency_form | expected a path, git, or version dependency |
| 14 | parse_manifest | new | E1011 | duplicate_dependency | duplicate package dependency |
| 15 | parse_manifest | new | E1017 | too_many_dependencies | a package may have at most 1024 dependencies |
| 16 | parse_manifest | lin | E0002 | unknown_manifest_key | unknown wasm key; expected max-memory or stack-size |
| 17 | parse_manifest | lin | E0002 | invalid_wasm_size | WASM sizes must be a quoted byte count or a number followe |
| 18 | parse_manifest | lin | E0002 | duplicate_manifest_key | wasm keys must occur at most once |
| 19 | parse_manifest | lin | E0002 | unknown_manifest_key | unknown registry key; expected index or rev |
| 20 | parse_manifest | lin | E0002 | invalid_registry_setting | <<rule>> |
| 21 | parse_manifest | lin | E0002 | duplicate_manifest_key | registry keys must occur at most once |
| 22 | parse_manifest | lin | E0002 | invalid_native_section | <<FORM>> |
| 23 | parse_manifest | lin | E0002 | invalid_native_library_name | <<&message>> |
| 24 | parse_manifest | lin | E0002 | invalid_native_path | link and search paths must be nonempty relative paths |
| 25 | parse_manifest | lin | E0002 | manifest_section_order | expected [package] before fields |
| 26 | parse_manifest | new | E0002 | missing_package_field | [package] requires name and version |
| 27 | parse_manifest | new | E0002 | missing_registry_index | [registry] requires index = "https://..." |
| 28 | parse_manifest | new | E0002 | unsupported_edition | unsupported edition '{value}'; supported editions: {} |
| 29 | parse_manifest | new | E1011 | invalid_package_namespace | package namespace must be identifiers joined by '::' such  |
| 30 | Line::expect | lin | E0002 | manifest_syntax_error | expected '{token}' |
| 31 | Line::key | lin | E0002 | manifest_syntax_error | expected a bare key |
| 32 | Line::string | lin | E0002 | manifest_syntax_error | unterminated string escape |
| 33 | Line::string | lin | E0002 | manifest_syntax_error | unescaped control character in string |
| 34 | Line::string | lin | E0002 | manifest_syntax_error | unterminated quoted string |
| 35 | Line::string_array | lin | E0002 | invalid_native_section | <<form>> |
| 36 | Line::string_array | lin | E0002 | invalid_native_section | <<form>> |
| 37 | Line::finish | lin | E0002 | manifest_syntax_error | unexpected content after manifest field |
| 38 | Line::path_dependency | lin | E0002 | invalid_dependency_path | dependency path must be a nonempty relative path |
| 39 | Line::path_dependency | lin | E0002 | unknown_manifest_key | only the path and native keys are supported in a dependenc |
| 40 | Line::path_dependency | lin | E0002 | invalid_dependency_native_flag | native must be true or false |
| 41 | Line::registry_dependency | lin | E0002 | invalid_registry_dependency | <<VERSION_FORM>> |
| 42 | Line::registry_dependency | lin | E0002 | invalid_registry_dependency | <<VERSION_RULE>> |
| 43 | Line::registry_dependency | lin | E0002 | invalid_registry_dependency | <<VERSION_FORM>> |
| 44 | Line::git_dependency | lin | E0002 | invalid_git_dependency | <<GIT_FORM>> |
| 45 | Line::git_dependency | lin | E0002 | invalid_git_dependency | <<GIT_URL_RULE>> |
| 46 | Line::git_dependency | lin | E0002 | invalid_git_dependency | <<GIT_FORM>> |
| 47 | Line::git_dependency | lin | E0002 | invalid_git_dependency | <<GIT_REV_RULE>> |
| 48 | Line::git_dependency | lin | E0002 | invalid_git_dependency | <<GIT_FORM>> |

### `src/computation.rs`（14 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | collect_all | new | E1018 | alias_outside_builder_file | '@alias {}' names a computation builder, so it belongs in  |
| 2 | collect_all | new | E1018 | class_outside_class_file | type class declarations belong in a .tt file; a .tt file m |
| 3 | collect_all | new | E1018 | declaration_in_class_file | a .tt file contains only type class declarations; put reco |
| 4 | collect_all | new | E2004 | builder_file_toplevel_code | a .tc file implements one builder named after the file; to |
| 5 | collect_all | new | E1022 | private_builder_operation | builder operation '{}' cannot be private; make the operati |
| 6 | collect_all | new | E1018 | builder_without_operations | a .tc builder needs at least one computation operation, su |
| 7 | lower | br | E1018 | unknown_computation_builder | unknown computation builder '{0}'; declare '@alias {0}' in |
| 8 | lower | br | E1018 | unknown_computation_builder | unknown computation builder '{}'; define its operations in |
| 9 | Lowering::call | new | E1018 | builder_missing_operation | computation builder '{}' does not define '{operation}' |
| 10 | bounded_depth | new | E0002 | computation_nesting_too_deep | computation expansion exceeds depth {MAX_NESTING}; split t |
| 11 | direct_block | new | E1018 | computation_without_builder | {what} needs a computation builder, but this body runs IO  |
| 12 | Checker::implicit_source_builder | new | E1018 | no_builder_for_value | no computation builder has a Bind operation for this value |
| 13 | Checker::implicit_root | new | E1018 | cannot_infer_builder | cannot infer the computation builder; add a result type an |
| 14 | Checker::lower_implicit_task_sequence | new | E1018 | implicit_task_needs_builder | this implicit task operation requires an explicit computat |

### `src/constants.rs`（13 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | limit | fail | E1026 | const_limit_exceeded | const evaluation exceeds the compiler limit; split or simp |
| 2 | validate | fail | E1026 | const_expression_not_allowed | this expression is not allowed in a phase-1 const; precomp |
| 3 | Checker::temporary_borrows | new | E1013 | const_temporary_borrow_escapes | a temporary const borrow cannot escape or cross applicatio |
| 4 | Evaluator::run | fail | E1026 | const_cycle | cyclic const definition; break the cycle or make one value |
| 5 | unary | fail | E1026 | const_unsupported_operation | this const unary operation is unsupported; use a runtime l |
| 6 | binary | fail | E1026 | const_evaluation_trapped | const division trapped on zero or signed overflow; use a n |
| 7 | binary | fail | E1026 | const_evaluation_trapped | const power trapped on a negative exponent; use a nonnegat |
| 8 | binary | fail | E1026 | const_unsupported_operation | unsupported const integer operation; use a runtime let |
| 9 | binary | fail | E1026 | const_unsupported_operation | unsupported binary float width |
| 10 | binary | fail | E1026 | const_unsupported_operation | this const arithmetic is unsupported; keep decimal arithme |
| 11 | float_binary | fail | E1026 | const_unsupported_operation | unsupported const floating-point operation; use a runtime  |
| 12 | to_float | fail | E1026 | const_unsupported_operation | decimal const casts are unsupported; use a runtime convers |
| 13 | cast | fail | E1026 | const_unsupported_operation | this const cast is unsupported; keep the conversion in run |

### `src/const_rules.rs`（18 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | check_signature | fail | E1026 | const_signature_invalid | a const def must have a concrete signature; remove type va |
| 2 | check_signature | fail | E1026 | const_signature_invalid | const def parameters and results must be plain values; '{} |
| 3 | rejected | fail | E1026 | const_operation_not_allowed | this operation is not allowed in a const def; move it to a |
| 4 | not_const_def | fail | E1026 | const_callee_not_const | '{name}' is not a const def; mark it 'const def' or comput |
| 5 | partial | fail | E1026 | const_partial_application | a const def must be fully applied in const evaluation; pas |
| 6 | operator_on_aggregate | fail | E1026 | const_aggregate_operator | an operator on a record, tuple, array, or union is not all |
| 7 | validate_function | fail | E1026 | const_unsupported_operation | this const cast is unsupported; keep the conversion in run |
| 8 | validate_function | fail | E1026 | const_borrow_not_allowed | a borrow is not allowed in a const def; read the value dir |
| 9 | validate_function | fail | E1026 | const_pipe_not_allowed | the pipe operator is not allowed in a const def; write the |
| 10 | validate_function | fail | E1026 | const_unsupported_operation | this const arithmetic is unsupported; keep decimal arithme |
| 11 | validate_function | fail | E1026 | const_for_loop_form | a for loop in a const def enumerates an array, a string, o |
| 12 | validate_function | fail | E1026 | const_assignment_target | a const def assigns only to a 'let mut' variable, not to a |
| 13 | validate_function | fail | E1026 | const_new_collection_form | new [T](n, f) in a const def takes a lambda or a const def |
| 14 | validate_function | fail | E1026 | const_callee_not_const | only a const def can be called in a const def; mark the ca |
| 15 | validate_function | fail | E1026 | const_function_value | a function value is not allowed in a const def; only new [ |
| 16 | validate_function | fail | E1026 | const_checked_operation | an '@checked' operation raises an exception that has no co |
| 17 | validate_function | fail | E1026 | const_aggregate_operator | comparing records, tuples, and arrays as a whole is not al |
| 18 | validate_function | fail | E1026 | const_unbuildable_type | a value of type '{}' cannot be built in a const def; use p |

### `src/const_machine.rs`（5 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | trapped | fail | E1026 | const_evaluation_trapped | const evaluation trapped: {message} |
| 2 | unsupported | fail | E1026 | const_operation_not_allowed | this operation is not allowed in a const def; move it to a |
| 3 | Run::constant | fail | E1026 | const_cycle | cyclic const definition; break the cycle or make one value |
| 4 | Run::truth | fail | E1026 | const_condition_not_bool | a const condition must be bool |
| 5 | Run::call | fail | E1026 | const_call_depth_limit | const evaluation exceeds the compiler limit of {MAX_NESTIN |

### `src/const_static.rs`（1 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | too_big | fail | E1026 | const_static_data_too_large | the constants that the program reads in place or borrows h |

### `src/regions.rs`（23 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | misplaced_quantifier | fail | E1013 | misplaced_region_quantifier | region-quantified function types are only supported as who |
| 2 | reject_local | fail | E1013 | named_region_in_local | named regions belong in function signatures or record decl |
| 3 | labels | fail | E1013 | named_region_in_function_type | named regions inside higher-order function types are not s |
| 4 | labels | fail | E1013 | multiple_regions_on_value | each borrowed value has one named region; split values wit |
| 5 | labels | fail | E1013 | region_on_non_borrow | a named region requires a reference or an aggregate carryi |
| 6 | undeclared | fail | E1013 | undeclared_region | undeclared region '{}'; add it after the declaration name |
| 7 | mixing | fail | E1013 | field_mixes_regions | one field cannot mix distinct named regions; give each fie |
| 8 | region_count_mismatch | fail | E1013 | region_count_mismatch | the record declares {count} regions; write exactly {count} |
| 9 | target_labels | fail | E1013 | exclusive_borrow_behind_region | the data behind a reference with its own target region can |
| 10 | field_regions | new | E1017 | too_many_record_regions | too many regions in record '{}'; declare at most {MAX_RECO |
| 11 | check_used | fail | E1013 | unused_region | unused region '{}'; remove it or use it in a borrowed type |
| 12 | exclusive_field_regions | fail | E1013 | exclusive_field_without_region | exclusive borrow field '{name}' needs a named region; decl |
| 13 | exclusive_field_regions | fail | E1013 | exclusive_field_regions_missing | field '{name}' stores an exclusive borrow inside record '{ |
| 14 | exclusive_positions | fail | E1013 | exclusive_region_reused | region '{}' belongs to an exclusive borrow, so only one fi |
| 15 | validate_modules | fail | E1013 | borrow_field_without_region | field '{}' stores a borrow without a region; name one of t |
| 16 | contract | fail | E1013 | mutable_region_quantified_parameter | a parameter with a region-quantified function type cannot  |
| 17 | result_sources | fail | E1013 | result_region_missing | annotate the whole borrowed result with one region, for ex |
| 18 | result_sources | fail | E1013 | result_region_without_input | result region '{region}' has no matching input; bind all f |
| 19 | callback_contract | fail | E1013 | region_quantifier_target | a region quantifier applies to a function type, as in '{r} |
| 20 | callback_contract | fail | E1013 | region_already_declared | region '{}' is already declared by the function; give the  |
| 21 | callback_contract | fail | E1013 | region_belongs_to_function | region '{}' belongs to the function, not to the quantified |
| 22 | callback_contract | fail | E1013 | result_region_missing | annotate the whole borrowed result with one region, for ex |
| 23 | validate_contract_calls | fail | E1013 | region_callback_stores_exclusive_input | '{name}' may store an input through an exclusive reference |

### `src/numeric.rs`（3 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | binary_bits | new | E1009 | invalid_float_literal | invalid floating-point literal |
| 2 | binary_bits | new | E1009 | float_literal_out_of_range | floating-point literal is out of range |
| 3 | decimal_literal | new | E1009 | decimal_literal_out_of_range | decimal literal is out of range |

### `src/formatter.rs`（2 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | format_source_with_edition | new | E2000 | formatter_invalid_output | formatter produced invalid syntax: {}; please report a bug |
| 2 | format_source_with_edition | new | E2000 | formatter_changed_semantics | formatter would change program semantics; please report a  |

