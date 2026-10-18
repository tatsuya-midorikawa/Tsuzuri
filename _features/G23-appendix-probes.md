# G23 付録: 誤りの形を HEAD に与えた結果（付録 E.4）

[G23 本体](G23-diagnostic-kinds-explain-hints.md) の「現状」と付録 E の元データです。`target/release/tsuzuri check <dir> --json` を、`Main.tz` にソースを 1 つ書いたディレクトリに実行しました（HEAD `9dd0fbbc`）。**89 件のうち、診断になったのは 63 件、受理されたのは 26 件**です。`⏎` は改行です。位置は最初の診断の `行:列`。

| id | ソース | 結果 |
| --- | --- | --- |
| `tuple_dot` | `def first :: (i64 * i64) -> i64 = \pair -> pair.0` | `E0002` 1:49 expected an identifier |
| `type_ascription` | `let x = (5: i64)` | `E0002` 1:11 expected the closing delimiter |
| `display_to_string` | `let s = Display.to_string 5` | `E1002` 1:17 type class 'Display' has no method 'to_string' |
| `new_binding` | `let new = 1` | `E0002` 1:5 expected an identifier |
| `abs_int` | `def f :: i64 -> i64 = \n -> abs n` | `E1003` 1:29 expected i64, found f64 |
| `task_if_return` | `def f :: bool -> Task<i64> = \c -> ⏎     task { ⏎         if c then return 1 ⏎         return 2 ⏎     }` | `E0002` 3:19 expected an expression |
| `fn_unit` | `def f :: unit -> i64 ⏎ fn f () = 1` | `E0002` 2:9 expected '->' and an explicit return type |
| `union_braces` | `union Reading { Missing, Value(i64) }` | `E0002` 1:15 expected '=' after the union name |
| `rust_fn` | `fn add(a: i64, b: i64) -> i64 { a + b }` | 受理（診断なし） |
| `range_for_braces` | `def f :: i64 -> i64 = \n -> ⏎     let mut t = 0 ⏎     for i in 0..n { t = t + i } ⏎     t` | `E0002` 3:19 expected 'do' after the enumerable expression |
| `println_macro` | `def f :: unit -> unit = \_ -> println!("x")` | `E1002` 1:31 unknown value 'println' |
| `compound_assign` | `def f :: i64 -> i64 = \n -> ⏎     let mut x = n ⏎     x += 1 ⏎     x` | `E0002` 3:8 expected an expression |
| `if_non_bool` | `def f :: i64 -> i64 = \n -> if n then 1 else 2` | `E1003` 1:32 expected bool, found i64 |
| `vec_new` | `def f :: unit -> i64 = \_ -> ⏎     let v = Vec::new() ⏎     0` | `E0002` 2:16 expected ';' or a newline after the binding |
| `rust_match` | `def f :: i64 -> i64 = \x -> match x { 1 => 2, _ => 3 }` | `E0002` 1:41 expected ';' or a newline after the binding |
| `return_in_def` | `def f :: i64 -> i64 = \x -> return x` | 受理（診断なし） |
| `string_plus` | `def f :: string -> string = \a -> a + "b"` | 受理（診断なし） |
| `plusplus` | `def f :: string -> string = \a -> a ++ "b"` | `E1005` 1:38 no instance for Numeric<string>; define an instance or use a supported |
| `method_len` | `def f :: [i64] -> i64 = \xs -> xs.len()` | `E1007` 1:35 field access requires a record, or '.length' on an array, list, string |
| `some_paren` | `def f :: i64 -> Maybe<i64> = \x -> Some(x)` | 受理（診断なし） |
| `literal_mismatch` | `let x: i64 = 5i32` | `E1003` 1:14 expected i64, found i32 |
| `missing_rec` | `def fact :: i64 -> i64 = \n -> if n <= 1 then 1 else n * fact (n - 1)` | `E1019` 1:5 recursive function 'Main.fact' requires 'rec' on its declaration and d |
| `use_after_move` | `def f :: unit -> i64 = \_ -> ⏎     let a = [1, 2] ⏎     let b = a ⏎     a.length + b.length` | 受理（診断なし） |
| `assign_immutable` | `def f :: unit -> i64 = \_ -> ⏎     let x = 1 ⏎     x = 2 ⏎     x` | `E1014` 3:5 mutable access requires 'let mut' or an exclusive reference ('ref mut' |
| `non_exhaustive` | `union T = A \| B ⏎ def f :: T -> i64 = \t -> match t with \| A -> 1` | `E1021` 2:27 match is not exhaustive; missing: B |
| `arg_count` | `def g :: i64 -> i64 -> i64 = \a b -> a + b ⏎ def f :: unit -> i64 = \_ -> g 1 2 3` | `E1006` 2:30 cannot apply 3 arguments to a function accepting 2 arguments |
| `use_std_import` | `use std::io;` | `E0002` 1:8 expected the end of the file; declarations must precede the entry-poin |
| `trait_decl` | `trait Shape { }` | `E1002` 1:1 unknown value 'trait' |
| `struct_decl` | `struct Point { x: i64 }` | `E1002` 1:1 unknown value 'struct' |
| `if_without_else` | `def f :: bool -> i64 = \c -> if c then 1` | `E1003` 1:40 expected i64, found unit |
| `closure_pipes` | `def f :: [i64] -> [i64] = \xs -> Array.map (\|x\| x + 1) xs` | `E0002` 1:45 expected an expression |
| `arrow_lambda` | `def f :: [i64] -> [i64] = \xs -> Array.map (x => x + 1) xs` | `E0002` 1:47 expected the closing delimiter |
| `null_value` | `def f :: unit -> i64 = \_ -> null` | `E1002` 1:30 unknown value 'null' |
| `interp_dollar` | `def f :: i64 -> string = \x -> $"v={x}"` | 受理（診断なし） |
| `call_parens` | `def g :: i64 -> i64 -> i64 = \a b -> a + b ⏎ def f :: unit -> i64 = \_ -> g(1, 2)` | 受理（診断なし） |
| `ternary` | `def f :: bool -> i64 = \c -> c ? 1 : 2` | `E0001` 1:32 unexpected character '?'; identifiers use ASCII |
| `let_in` | `def f :: unit -> i64 = \_ -> let x = 1 in x` | `E0002` 1:30 expected an expression |
| `semicolon_end` | `let x = 1;` | 受理（診断なし） |
| `bool_and` | `def f :: bool -> bool -> bool = \a b -> a and b` | `E1019` 1:47 'and' requires a preceding recursive declaration or definition |
| `not_equal` | `def f :: i64 -> bool = \a -> a != 1` | 受理（診断なし） |
| `else_if_multiline` | `def f :: i64 -> i64 = \n -> ⏎     if n < 0 then ⏎         -1 ⏎     else ⏎         if n == 0 then ⏎            ` | 受理（診断なし） |
| `while_loop` | `def f :: i64 -> i64 = \n -> ⏎     let mut i = 0 ⏎     while i < n { ⏎         i = i + 1 ⏎     } ⏎     i` | `E0002` 3:17 expected 'do' after the while condition |
| `colon_signature` | `def f: i64 -> i64 = \n -> n` | `E0002` 1:6 use '::' between a 'def' name and its type, as in 'def name :: Type' |
| `lambda_arrow_old` | `def f :: i64 -> i64 = n -> n` | 受理（診断なし） |
| `mut_param` | `def f :: i64 -> i64 = \mut n -> n` | 受理（診断なし） |
| `string_index` | `def f :: string -> char = \s -> s[0]` | `E1003` 1:33 expected char, found i16u |
| `char_single_quote_string` | `def f :: unit -> string = \_ -> 'abc'` | `E0001` 1:33 a char literal contains exactly one UTF-16 code unit |
| `option_type` | `def f :: Option<i64> -> i64 = \o -> 0` | `E1004` 1:10 unknown type class, record type, or union type 'Option'; declare it or |
| `pub_def` | `pub def f :: i64 -> i64 = \x -> x` | `E0002` 1:5 expected the end of the file; declarations must precede the entry-poin |
| `const_decl` | `const X: i64 = 5` | 受理（診断なし） |
| `impl_block` | `impl Point { }` | `E1002` 1:1 unknown value 'impl' |
| `try_question` | `def g :: unit -> Result<i64, string> = \_ -> Ok 1 ⏎ def f :: unit -> Result<i64, string> = \_ -> Ok (g ()? + 1` | `E0001` 2:54 unexpected character '?'; identifiers use ASCII |
| `bang_not` | `def f :: bool -> bool = \a -> !a` | 受理（診断なし） |
| `percent_mod` | `def f :: i64 -> i64 = \a -> a % 2` | 受理（診断なし） |
| `array_push` | `def f :: unit -> i64 = \_ -> ⏎     let mut xs = [1, 2] ⏎     xs.push(3) ⏎     0` | `E1007` 3:8 field access requires a record, or '.length' on an array, list, string |
| `generic_angle_def` | `def id<T> :: T -> T = \x -> x` | `E0002` 1:7 expected '::' after the declaration name |
| `def_params_rust` | `def f(x: i64): i64 = x` | `E0002` 1:6 expected '::' after the declaration name |
| `for_in_exclusive` | `def f :: i64 -> i64 = \n -> ⏎     let mut t = 0 ⏎     for i in 0 .. n do ⏎         t = t + i ⏎     t` | 受理（診断なし） |
| `else_if_oneline` | `def f :: i64 -> i64 = \n -> if n < 0 then -1 else if n == 0 then 0 else 1` | 受理（診断なし） |
| `elif_ok` | `def f :: i64 -> i64 = \n -> ⏎     if n < 0 then ⏎         -1 ⏎     elif n == 0 then ⏎         0 ⏎     else ⏎  ` | 受理（診断なし） |
| `class_rust_trait_impl` | `class Shape<'a> = ⏎     def area :: 'a -> f64` | `E0002` 1:17 expected '{' after the class parameter |
| `record_new_syntax` | `record Point { x: i64, y: i64 } ⏎ let p = Point(1, 2)` | `E1002` 2:9 unknown value 'Point' |
| `record_field_equals` | `record Point { x: i64, y: i64 } ⏎ let p = Point { x = 1, y = 2 }` | `E0002` 2:22 expected ';' or a newline after the binding |
| `record_missing_field` | `record Point { x: i64, y: i64 } ⏎ let p = Point { x: 1 }` | `E1007` 2:9 missing fields: y |
| `shadow_type_i32_default` | `def f :: unit -> i64 = \_ -> 5 + 3000000000` | 受理（診断なし） |
| `string_len` | `def f :: string -> i64 = \s -> String.len s` | `E1002` 1:39 module 'String' has no function or union case 'len' |
| `array_len_fn` | `def f :: [i64] -> i64 = \xs -> len xs` | `E1002` 1:32 unknown value 'len' |
| `map_syntax` | `def f :: [i64] -> [i64] = \xs -> xs.map(\x -> x + 1)` | `E1007` 1:37 field access requires a record, or '.length' on an array, list, string |
| `return_unit_semicolon` | `def f :: unit -> unit = \_ -> ();` | 受理（診断なし） |
| `match_no_with` | `def f :: i64 -> i64 = \x -> match x \| 1 -> 2 \| _ -> 3` | `E0002` 1:41 expected 'with' after the matched expression |
| `match_arrow_fat` | `def f :: i64 -> i64 = \x -> match x with \| 1 => 2 \| _ => 3` | `E0002` 1:46 expected '->' after the pattern and optional guard |
| `match_missing_bar` | `def f :: i64 -> i64 = \x -> ⏎     match x with ⏎     1 -> 2 ⏎     _ -> 3` | `E0002` 3:5 expected at least one '\| pattern -> expression' arm |
| `wildcard_binding_unused` | `def f :: i64 -> i64 = \x -> ⏎     let y = 3 ⏎     x` | 受理（診断なし） |
| `if_cond_parens_then_brace` | `def f :: i64 -> i64 = \x -> if (x > 1) { 1 } else { 2 }` | 受理（診断なし） |
| `lambda_multi_arg_tuple` | `def f :: i64 -> i64 -> i64 = \(a, b) -> a + b` | `E1003` 1:31 expected (an undetermined type * an undetermined type), found i64 |
| `assign_arrow` | `def f :: unit -> i64 = \_ -> ⏎     let mut x = 0 ⏎     x <- 1 ⏎     x` | `E1003` 3:5 expected unit, found bool |
| `ref_param_symbol` | `def f :: &i64 -> i64 = \x -> *x` | 受理（診断なし） |
| `ref_arg_missing` | `def g :: ref [i64] -> i64 = \xs -> xs.length ⏎ def f :: unit -> i64 = \_ -> ⏎     let a = [1, 2] ⏎     g a + g` | 受理（診断なし） |
| `move_in_loop` | `def g :: [i64] -> i64 = \own xs -> 0 ⏎ def f :: unit -> i64 = \_ -> ⏎     let a = [1, 2] ⏎     g a + g a` | `E1006` 1:5 signature expects 1 parameters, definition has 2 |
| `module_dot_call` | `def f :: unit -> i64 = \_ -> Math.sqrt2 4.0` | `E1002` 1:35 module 'Math' has no function or union case 'sqrt2' |
| `unknown_type` | `def f :: Int -> Int = \x -> x` | `E1004` 1:10 unknown record, union, or type alias 'Int' |
| `int_type_name` | `def f :: int -> int = \x -> x` | `E1004` 1:10 unknown record, union, or type alias 'int' |
| `float_type_name` | `def f :: float -> float = \x -> x` | `E1004` 1:10 unknown record, union, or type alias 'float' |
| `str_type_name` | `def f :: str -> str = \x -> x` | `E1004` 1:10 unknown record, union, or type alias 'str' |
| `char_literal_double` | `def f :: unit -> char = \_ -> "a"` | `E1003` 1:31 expected char, found string |
| `bool_literals_caps` | `def f :: unit -> bool = \_ -> True` | `E1002` 1:31 unknown value 'True' |
| `list_literal_bracket_pipe` | `def f :: unit -> [i64] = \_ -> [1, 2, 3]` | 受理（診断なし） |
| `unit_value` | `def f :: unit -> unit = \_ -> void` | `E1002` 1:31 unknown value 'void' |
| `rec_missing_and` | `def rec a :: i64 -> i64 = \n -> b n ⏎ def rec b :: i64 -> i64 = \n -> a n` | 受理（診断なし） |
