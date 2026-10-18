# G23 付録 E: 補助関数・特別な箇所・数え方・厳選 kind・試作の結果

[G23 本体](G23-diagnostic-kinds-explain-hints.md) の付録 E です。E.4（誤りの形 89 件）は [G23-appendix-probes.md](G23-appendix-probes.md)。

## E.1 コードを引数に取る補助関数・コードを内側に持つ補助関数の変更

「その場」は、補助関数のシグネチャをその手順で変える（コンパイラが呼び出しをすべて指す）。「兄弟」は、`kind` を取る新しい関数を足して旧関数を `#[deprecated]` にし、全部移したら旧関数を消す（呼び出しが複数の手順にまたがるため）。

| # | ファイル | 補助関数（現在） | 移行後 | 方式・手順 |
| --- | --- | --- | --- | --- |
| 1 | `src/diagnostic.rs` | `Diagnostic::new(code, message, span)`・`Diagnostic::warning(code, message, span)` | `#[deprecated]`（2.1）、削除（2.20）。代わりに `Diagnostic::of(kind, message, span)` | — |
| 2 | `src/parser.rs`（`parse_control.rs` も使う） | `Parser::error(&self, message: impl Into<String>) -> Diagnostic`（内側のコード `E0002`、位置は現在のトークン） | `Parser::error_of(&self, kind: DiagnosticKind, message: impl Into<String>) -> Diagnostic` | 兄弟。2.3 で足し 2.5 で旧関数を削除 |
| 3 | `src/parser.rs` | `Parser::rec_error<T>(&self, message: &str, span: Option<Span>) -> Result<T, Diagnostic>`（`E0002`） | `rec_error_of<T>(&self, kind, message: &str, span: Option<Span>)` | 兄弟。2.3〜2.5 |
| 4 | `src/package.rs` | `Line::error(&self, message: &str) -> Diagnostic`（`E0002`） | `Line::error(&self, kind: DiagnosticKind, message: &str)` | その場。2.6（`self.error(` 19・`cursor.error(` 15） |
| 5 | `src/constants.rs` | `pub(super) fn failure(span: Span, message: impl Into<String>) -> Diagnostic`（`E1026`） | `failure(kind: DiagnosticKind, span: Span, message: impl Into<String>)` | その場。2.7（呼び出し: `constants.rs`・`const_rules.rs`・`const_machine.rs`・`const_static.rs` と `check.rs` の 1 行） |
| 6 | `src/regions.rs` | `fn failure(span, message)`（`E1013`） | `failure(kind, span, message)` | その場。2.7 |
| 7 | `src/const_machine.rs`・`src/constants.rs` | `unsupported(span)`・`trapped(span, message)`・`limit(span)`（固定の 1 条件） | 変えない（中の `failure` の呼び出しに kind を足す: `const_operation_not_allowed`・`const_evaluation_trapped`・`const_limit_exceeded`） | 2.7 |
| 8 | `src/gpu.rs`（`gpu/spirv.rs` が `use super::{GpuKernel, unsupported}`） | `fn unsupported(message: &str, span: Span) -> Diagnostic`（`E1018`） | `unsupported(kind: DiagnosticKind, message: &str, span: Span)` | その場。2.13 |
| 9 | `src/ownership.rs`（`ownership_control.rs` も使う） | `fn error(code: &'static str, message: impl Into<String>, span: Span) -> Diagnostic` | `error(kind: DiagnosticKind, message: impl Into<String>, span: Span)`（本体は `Diagnostic::of`） | その場。2.8 |
| 10 | `src/fetch.rs` | `Location::error(&self, code: &'static str, message: String) -> SourceError` | `error(&self, kind: DiagnosticKind, message: String)` | その場。2.14 |
| 11 | `src/fetch.rs` | `fn parse_tree(…) -> Result<Vec<TreeEntry>, (&'static str, String)>`、その中の closure `unreadable`・`limits` | `Result<Vec<TreeEntry>, (DiagnosticKind, String)>`、呼び出し側の `.map_err(\|(code, message)\| location.error(code, message))` は `(kind, message)` | その場。2.14（付録 C の `man` 行 10 本） |
| 12 | `src/driver.rs`（`test_runner.rs`・`bindgen_driver.rs` も使う） | `fn driver_error(code: &'static str, message: impl Into<String>) -> Diagnostic`（位置なし） | `driver_error_of(kind: DiagnosticKind, message: impl Into<String>) -> Diagnostic` | 兄弟。2.15 で足し 2.17 で旧関数を削除 |
| 13 | `src/driver.rs` | `fn io_error(action, path, error)`（固定 `E2001`）、`spawn_error` | 本体を `driver_error_of(DiagnosticKind::IoFailure, …)` に（呼び出し 113 は変えない） | 2.16 |
| 14 | `src/driver.rs` | `load_packages` の closure `error = \|code: &'static str, message: String\|`、`offline_package` の closure `error = \|message\|`（固定 `E2007`） | 第 1 引数を `DiagnosticKind` に（前者）、kind 引数を足す（後者） | その場。2.15・2.16 |
| 15 | `src/repl.rs` | `fn command_error(errors, line, message)`（`E2000`） | `command_error(errors, line, kind, message)`（呼び出し 4: `repl_command_usage` ×3、`repl_unknown_command`） | その場。2.17 |
| 16 | `src/lexer.rs` | `byte_suffix` の closure `error = \|message: &str\|` | `\|kind: DiagnosticKind, message: &str\|` | その場。2.3 |
| 17 | `src/main.rs` | `fetch_dependencies`・`publish_package` の closure `usage = \|message: &str\|` | `\|kind: DiagnosticKind, message: &str\|`（呼び出し 6） | その場。2.19 |

## E.2 `Diagnostic` の作り方が特別な箇所（付録 C の `br`・`man` 行）

| # | 場所 | 現在 | 移行後 |
| --- | --- | --- | --- |
| 1 | `check.rs` `record_arity` | `let code = if matches!(named, NamedType::Alias(_)) { "E1024" } else { "E1004" }; Err(Diagnostic::new(code, message, span))` | `let kind = if … { DiagnosticKind::AliasArgumentCount } else { DiagnosticKind::TypeArgumentCount }; Err(Diagnostic::of(kind, message, span))`（付録 C の `br` 2 行） |
| 2 | `check.rs` `resolve_type_with_kinds` | `record_arity(…).map_err(\|mut error\| { if !matches!(named, NamedType::Alias(_)) { error.code = "E1015"; } error })?` | `.map_err(\|error\| if matches!(named, NamedType::Alias(_)) { error } else { error.recode(DiagnosticKind::MissingTypeArguments) })?` |
| 3 | `polymorph.rs` `solve_families` | `Diagnostic::new(if is_unknown(&input) { "E1015" } else { "E1005" }, "this SIMD operation needs …", family.span)` | `Diagnostic::of(if is_unknown(&input) { DiagnosticKind::SimdVectorTypeUnknown } else { DiagnosticKind::SimdOperationVectorType }, …)` |
| 4 | `polymorph.rs` `resolve_constraints` | `self.resolve(names, module, name).map_err(\|mut error\| { error.code = "E1027"; error })?` | `.map_err(\|error\| error.recode(DiagnosticKind::UnknownClassInContext))?` |
| 5 | `polymorph.rs` インスタンスの検証 | `if error.code != "E1017" { error.code = if instance.derived { "E1025" } else { "E1027" }; }` | `if error.code != "E1017" { error = error.recode(if instance.derived { DiagnosticKind::DerivedInstanceInvalid } else { DiagnosticKind::InstanceCheckFailed }); }`（`error` を `mut` で束縛し直す） |
| 6 | `polymorph.rs` `derived_error` | `error.code = "E1025"; error.span = instance.span;` | `error = error.recode(DiagnosticKind::DerivedInstanceInvalid); error.span = instance.span;` |
| 7 | `parser.rs` `alias_declaration` | `let message = if … { Some(String) } else if … { Some(format!(…)) } …; if let Some(message) = message { return Err(Diagnostic::new("E0002", message, name.span)); }` | `Some((DiagnosticKind::BuilderAliasNameCase, message))`・`(BuilderAliasIsContextualKeyword, …)`・`(DuplicateBuilderAlias, …)` の組にして `Diagnostic::of(kind, message, name.span)` |
| 8 | `computation.rs` `lower` | 2 つのメッセージの `let message = if …`（先頭が小文字かどうか） | 規則が同じなので 1 kind `UnknownComputationBuilder`（`Diagnostic::of(…, message, builder.span)`） |
| 9 | `lexer.rs` `byte_suffix`・`character` | closure `error` が 3 つのメッセージを受ける（`byte_suffix`）、固定メッセージの closure を 6 回呼ぶ（`character`） | `byte_suffix` は kind 引数を足して呼び出し 3 に `ByteLiteralNotAscii`・`ByteLiteralNotAscii`・`ByteSuffixOnUnsupportedLiteral`。`character` の closure は固定の `Utf8charNotOneScalar` |
| 10 | `lexer.rs` `parse_format_spec` | closure `invalid = \|message: String\| Diagnostic::new("E0001", message, span)` と、`malformed`・`out_of_range` の 2 つの closure、`invalid(format!(…))` の 2 回 | `invalid` に kind 引数: `malformed` → `InvalidFormatSpec`、`out_of_range` → `FormatFieldOutOfRange`、整数の精度 → `FormatPrecisionOnInteger`、精度の欠落 → `FormatTypeNeedsPrecision`（付録 C の `br` 4 行） |
| 11 | `package.rs` `create_project` | closure `error = \|message: String\| Diagnostic::new("E2000", message, Span::default())` を 4 つの条件で呼ぶ | 第 1 引数に kind: `InvalidNamespaceArgument`・`CannotDeriveNamespace`・`NewProjectIoFailure`・`NewProjectFolderNotEmpty`（付録 C の `br` 4 行） |
| 12 | `main.rs` `run_test_action`・`run_bench_action` | `project.source_for(&Diagnostic::new("E2000", "", case.span))`（位置から元ファイルを引くためだけ） | `Project::source_for_span(&self, span: Span) -> &SourceFile`（`src/driver.rs` に足し、`source_for` はそれを呼ぶ）。2 箇所とも `project.source_for_span(case.span)` |
| 13 | `main.rs` `fetch_dependencies`・`publish_package` | closure `usage(message)`（`E2000`）を 6 回 | E.1 の 17。kind: `usage_error`・`project_manifest_missing`・`invalid_publish_git_url`・`invalid_publish_git_rev` |
| 14 | `main.rs` `main`（`options.validate()` の失敗） | `Diagnostic::new(error.code, format!("{} (after applying the root package's [wasm])", error.message), Span::default())` | `let mut error = error; error.message = format!("{} (after applying the root package's [wasm])", error.message); error.span = Span::default();`（`kind`・`code` を保つ。G22 の `Deref` でも動く書き方） |
| 15 | `main.rs` 手作りの JSON 5 行 | `"{{\"severity\":\"warning\",\"code\":\"W2001\",\"message\":{}}}"` ×4（`run_test_action`・`debug_test`・`run_bench_action`・`main`）、`"{{\"severity\":\"error\",\"code\":\"E2000\",\"message\":\"file is not formatted\",\"path\":{}}}"`（`run_formatter`） | メッセージとパスの後ろに `,\"kind\":\"tool_notice\"`（`W2001`）と `,\"kind\":\"file_not_formatted\"` を足す（キーの順序: 既存のキーの次。`span` のない行なので `message`・`path` の次）。kind は登録簿に `ToolNotice`（`W2001`）・`FileNotFormatted`（`E2000`） |
| 16 | `main.rs` `bindgen_command`・`repl_command` | `Err(message) => print_diagnostic(&Diagnostic::new("E2000", message, Span::default()), …)`（`bindgen::parse_arguments` の文字列、`parse_repl_arguments` の文字列） | `bindgen_command` は `Diagnostic::of(DiagnosticKind::BindgenArgumentError, message, Span::default())`（`bindgen::parse_arguments` は `String` のまま）。`repl_command` は `parse_repl_arguments` が返す `Diagnostic` を直接表示 |
| 17 | `main.rs` CLI の解析 5 関数 | `parse_arguments`・`next_value`・`parse_size`・`parse_repl_arguments`・`script_arguments` が `Result<_, String>` | `Result<_, Diagnostic>`。補助 `fn command_line_error(kind: DiagnosticKind, message: impl Into<String>) -> Diagnostic`（`Diagnostic::of(kind, message, Span::default())`）。`return Err("… specified more than once".into())` → `return Err(command_line_error(DiagnosticKind::DuplicateOption, "… specified more than once"))`。`main` の 2 か所（`script_arguments`・`parse_arguments` の失敗）は `Err(error) => print_diagnostic(&error, …)` |
| 18 | `main.rs` `.ok_or("…")?`・`options.validate()`・`parse_target` | 文字列の `?`、`return Err(error.message)`（`validate()` の診断を文字列にしていた） | `.ok_or_else(\|\| command_line_error(DiagnosticKind::OptionInvalidValue, "…"))?`、`validate()` の `Diagnostic` はそのまま `?`（kind を保つ）、`tsuzuri::target::parse_target(…).map_err(\|message\| command_line_error(DiagnosticKind::UnknownTarget, message))?`。`src/main.rs` の inline テストの `parse`・`parse_repl` は `.map_err(\|error\| error.message.clone())` を足し、期待値（文字列）は変えない |
| 19 | `driver.rs` `run_process`・`capture` | `Err(driver_error("E2005", message))`・`Diagnostic::new("E2005", message, …)` の `message` が `if` で選ばれる | 規則が同じなので `ProgramExitedNonzero`・`ProgramTerminated`・`ProgramTimeLimitExceeded` の 1 kind ずつ（付録 C の行） |
| 20 | `fetch.rs` `download` | `location.error(code, message)`（`parse_tree` の `(code, message)` を受ける） | `location.error(kind, message)`（E.1 の 11） |
| 21 | テストが任意のコードの診断を作る 11 箇所 | `tests/diagnostics.rs` 518〜520・551（`E1003`・`E1002`）、`tests/warnings.rs` 161・174・178（`W1001`・`E1003`）、`src/diagnostic.rs` の 2 つのテスト（`E0001`・`W1003`）、`src/check.rs` の `verify_children` のテスト（`E1017`）、`src/recursive.rs` の `cache_prints_as_the_refcell_it_replaced_and_clones_its_entries`（`E1010`） | 手順 2.20 の対応表（`TypeMismatch`・`UnknownValue`・`UnusedLocal`・`UnexpectedCharacter`・`UnreachableMatchArm`・`PolymorphicTypeExpansionLimit`・`InfiniteRecursiveLayout`）。`W1003` の kind は `unreachable_match_arm`。`deny(deprecated)` は `#[cfg(test)]` の中も指すので、`check.rs`（`E1017`）は手順 2.12、`recursive.rs`（`E1010`）は手順 2.2 で直し、残り 9 箇所は手順 2.20 |
| 22 | `repl.rs` `classify` | `Err(Diagnostic::new("E2000", message, span))`（`message` は宣言の種類で選ぶ定数） | 規則が同じなので 1 kind `ReplDeclarationNotAllowed` |

## E.3 数え方（再現）

付録 C の行は、`src/` を `grep` と、`fn`・`impl` の範囲を見て各行がどの関数にあるかを決める補助スクリプトで数え（`Diagnostic::new`／`warning`・`driver_error`・`error`・`Location::error`・`self.error`・`rec_error`・`failure`・`unsupported`・closure の呼び出しのうち、コードの引数・コードを内側に持つ補助のもの）、テストの区間（`#[cfg(test)] mod tests` の後）を除いたうえで、行ごとに人が kind を決めた。次のコマンドで同じ数に当たる。

| 何を | コマンド | 結果 |
| --- | --- | --- |
| 非テストの `Diagnostic::new`／`warning` の行 | `grep -rn 'Diagnostic::new(\|Diagnostic::warning(' src \| wc -l` から inline テスト 4 行を引く | 483 − 4 = 479（`new` 471 = 付録 C の `new` 行 444 ＋ 分岐の元 6（付録 C では分岐ごとの `br` 行 17 に置き換えた）＋ 補助の定義 21。`warning` 8） |
| コードのリテラルの行 | `grep -rE '"[EW][0-9]{4}"' src \| wc -l` | 868 行（879 個。比較・テストを含む） |
| 付録 C の行の合計 | 表の行を数える | 1,077 |
| 付録 C が行にしない補助の定義 | E.1 の関数の定義（コードを持つ本体）と分岐の元 | 29 箇所（`new` 27〈分岐の元 6 を含む〉・`Location::error` 1・`Parser::rec_error` 1） |

**付録 C にない発行箇所が見つかったとき**: コンパイラ（`#[deprecated]` の警告）が指す。S1 の決定表で kind を決め、付録 B.1 の形で登録簿へ足し、`src/` の `DiagnosticKind::…` を書く。1 手順で 10 行を超えたら停止条件 3。

## E.5 厳選 kind 84（手順 2.22〜2.25 のページ。☆ は Phase 1 の 8）

- 構文（22）: `expected_token`・`expected_identifier`・`expected_expression`・`expected_pattern`・`expected_binding_end`・`expected_line_break`・`unexpected_character`・`unterminated_string_literal`・`invalid_string_escape`・`char_literal_not_one_unit`・`invalid_numeric_literal`・`source_too_large`・`syntax_nesting_too_deep`・`match_without_arms`・`definition_without_signature`・`signature_without_definition`・`let_function_needs_lambda`・`misplaced_doc_comment`・`signature_uses_def`・`signature_uses_double_colon`・`mut_as_expression`・`slice_must_be_borrowed`
- 名前と型（28）: `duplicate_name`・`duplicate_function_definition`・`unknown_value`☆・`unknown_function`・`unknown_module_member`☆・`unknown_type_name`・`unknown_field`・`unknown_union_case`・`ambiguous_name`・`type_mismatch`☆・`call_non_function`・`too_many_arguments_applied`・`definition_parameter_count_mismatch`・`record_has_no_field`・`missing_record_fields`・`field_access_non_record`・`integer_literal_out_of_range`・`ambiguous_polymorphic_use`・`concrete_type_required`・`infinite_inferred_type`・`no_instance`・`instance_missing_method`・`overlapping_instance`・`class_has_no_method`・`index_non_indexable`・`deref_non_reference`・`type_argument_count`・`empty_list_needs_annotation`
- 所有権（11）: `use_after_move`☆・`move_out_of_collection_element`・`assign_to_immutable_place`・`borrow_conflict`☆・`mutate_immutable_value`☆・`mutate_through_shared_reference`・`borrow_outlives_block`☆・`borrow_of_temporary`・`task_capture_retains_borrow`・`reference_without_owner`・`update_drop_type_value`
- パターンと再帰（5）: `non_exhaustive_match`☆・`or_pattern_binding_mismatch`・`case_pattern_missing_payload`・`and_without_recursive_group`・`missing_rec_marker`
- モジュールと可視性（5）: `private_name_access`・`invalid_module_name`・`duplicate_module`・`reserved_std_namespace`・`private_type_in_public_api`
- 警告（6）: `unused_local`・`unused_private_function`・`unreachable_match_arm`・`shadowed_local`・`deprecated_use`・`implicit_copy_of_large_value`
- ツール（7）: `option_requires_build`・`option_conflict`・`unknown_option`・`duplicate_option`・`missing_input`・`entry_point_missing`・`input_extension_invalid`

選んだ基準: 利用者（AI を含む）が書いたプログラムが当たりやすく、メッセージだけでは直し方が決まりにくい kind（`E0002` の構文・名前と型・所有権・パターン）と、既存のテストやヒントがすでに例を持つ kind。警告のうち既定で無効なもの（`shadowed_local` は `W1004`、`implicit_copy_of_large_value` は `W1006`＝`--warn implicit-copy`）は `text` の例にする。

## E.6 試作の結果（`rustc 1.98.1`、edition 2024、HEAD `9dd0fbbc`）

- **登録簿のマクロ**（本体の S2）を単独のファイルで `rustc --edition 2024` にかけ、3 つの kind（2 つの `since` の群）で `ALL`・`id`・`code`・`since`・`severity`・`from_id` が期待どおりに動くことを確かめた（`MSRV 1.85` で使える構文だけ）。
- **モジュール単位の `deny(deprecated)`**: クレートの先頭に `#![allow(deprecated)]`、`#[deprecated]` の関数を呼ぶ 2 つのモジュールのうち一方の先頭に `#![deny(deprecated)]` を置くと、**その一方だけ**が `error: use of deprecated associated function` になり、他方はそのまま通った。
- **ヒント 40 件**: `target/release/tsuzuri check --json` で、全 40 件の `bad` が指定のコードとメッセージの断片を出し、全 `good` がエラーなしで通った。
- **ページ 8 本**: 付録 A の `Bad` が指定のコード（`E1012`・`E1014`・`E1003`・`E1002`・`E1021`・`E1013`）を出し、`Good` が通った。`See also` の kind はすべて付録 B にある。
- **ヒントの `patterns`**: 付録 D の 40 件の `patterns` を、`src/lexer.rs` の `identifier`（キーワード表）と `symbol`（記号表）を写した字句の近似（Python）で切ったトークンと、`check --json` が返す主スパンの先頭に当てた。40 件すべての `bad` で、診断の主スパンの先頭のトークンを `@` に置いた `patterns` のどれかが合った（疑似トークンは `question-mark` の `?` だけ）。
- **試作しなかったもの**: Rust の `src/hints.rs`・`src/explain.rs` の実装（このリポジトリで `cargo build` を走らせない取り決めのため）。実装者が最初に確かめること: ① 字句の近似ではなく実際の `lex_all` が付録 D の各 `bad` のトークンを `patterns` の想定どおりに切ること（特に `pair.0` の `0` が `Integer` か、`+=` が `+` と `=` か）、② `LazyLock` の索引、③ `include_str!` の相対パス。

## E.7 Phase 2 の手順の割り当て（ファイルと行の範囲）

行の範囲は付録 C のファイル内の通し番号。合計は 1,077 行（44 ファイル）。**子モジュールは親より先に移す**（本体の「Phase 2 の移行のしくみ」）。

| 手順 | ファイル（行） | 補助関数・特記 |
| --- | --- | --- |
| 2.1 | 登録簿の全 653 kind、`recode`、`#[deprecated]`、`allow` | `tests/diagnostic_kinds.rs` の検査を足す |
| 2.2 | 小さなモジュール 20 個（計 53 行。`lib.rs` はクレートの根なので `deny` を置かない）: `numeric`・`formatter`・`abi`・`trap`・`recursion`・`parallel`・`llvm_traps`・`llvm_ranges`・`llvm_cpu`・`lib`・`docgen`・`copies`・`warnings`・`exhaustiveness`・`exceptions`・`derive`・`higher_kinds`・`recursive`・`closures`・`closures_rec` | `copies.rs`・`warnings.rs` は `Diagnostic::warning`。`closures` は子 `closures_rec` と同じ手順で移す。`recursive.rs` の inline テストの `E1010` もここで直す |
| 2.3 | `lexer.rs`（29）、`parse_control.rs`（22） | `Parser::error_of`・`rec_error_of` を足す（E.1） |
| 2.4 | `parser.rs` 1〜60 | |
| 2.5 | `parser.rs` 61〜121、旧 `Parser::error`・`rec_error` を消す | `parser.rs`・`parse_control.rs` に `deny` |
| 2.6 | `package.rs`（48）、`computation.rs`（14） | `Line::error` はその場で |
| 2.7 | `constants.rs`（13）、`const_rules.rs`（18）、`const_machine.rs`（5）、`const_static.rs`（1）、`regions.rs`（23）、`check.rs` の 47 番（`constants::failure` を呼ぶ 1 行。補助の型が変わるのでここで直す） | `failure` はその場で。`constants.rs` に `deny` |
| 2.8 | `control.rs`（20）、`ownership.rs`（41）・`ownership_control.rs`（4）、`llvm.rs`（15） | `ownership::error` はその場で |
| 2.9 | `polymorph.rs` 1〜51（`recode` の 19・34・35・36 番を含む） | |
| 2.10 | `polymorph.rs` 52〜102（コードを条件で選ぶ 76・77 番を含む） | |
| 2.11 | `check.rs` 1〜70（47 番を除く） | |
| 2.12 | `check.rs` 71〜139（`recode` の 75 番、`record_arity` の 87・88 番を含む）。`check.rs` とその子すべてに `deny` が揃う | inline テストの `E1017` もここで直す |
| 2.13 | `gpu.rs`（27）、`gpu/spirv.rs`（27） | `unsupported` はその場で。`gpu.rs` に `deny` |
| 2.14 | `fetch.rs`（38） | `Location::error`・`parse_tree` はその場で（E.1 の 10・11、E.2 の 20） |
| 2.15 | `driver.rs` 1〜73 | `driver_error_of` を足す（E.1 の 12・14） |
| 2.16 | `driver.rs` 74〜146 | `io_error` は `driver_error_of(IoFailure, …)`（E.1 の 13、E.2 の 19） |
| 2.17 | `test_runner.rs`（36）、`bindgen_driver.rs`（16）、`repl.rs`（9）、旧 `driver_error` を消す | `driver.rs` に `deny`（E.1 の 15、E.2 の 22） |
| 2.18 | `main.rs` 1〜83（`parse_arguments`・`next_value`・`parse_size`。`Result<_, String>` を `Result<_, Diagnostic>` にする） | E.2 の 17・18 |
| 2.19 | `main.rs` 84〜110（`parse_repl_arguments`・`script_arguments`、`usage` の closure、直接の発行箇所、手作りの JSON 5 行）、`source_for_span`、`bindgen_command`・`repl_command`・`main` の補助 | E.1 の 17、E.2 の 12〜16 |
| 2.20 | 残りのテストの構築 9 箇所（`src/diagnostic.rs` の 2、`tests/diagnostics.rs` の 4、`tests/warnings.rs` の 3）、`Diagnostic::new`・`warning` を消す、全 `allow`・`deny` の `deprecated` を消す | E.2 の 21 |
| 2.21 | 全 47 コードのページ（`src/explain/codes/`） | `CODE_PAGES` |
| 2.22〜2.25 | 厳選 84 kind のうち Phase 1 の 8 を除く 76 kind のページ（E.5。20 本ずつ） | `KIND_PAGES` |
| 2.26 | 文書 | 本体の「ドキュメント」の一覧 |
