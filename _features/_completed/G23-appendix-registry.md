# G23 付録: 登録簿（`src/diagnostic_kinds.rs` に貼る全 653 kind）と kind の一覧

[G23 本体](G23-diagnostic-kinds-explain-hints.md) の付録 B です。このファイルは、HEAD の発行箇所の表（`G23-appendix-sites-*.md`）から機械的に作りました。
件数: kind 653（発行箇所の表から 649 と、本チケットが足す `unknown_explain_target`）、コード 47、発行箇所の行 1077。

## B.1 登録簿のブロック（手順 2.1 でそのまま貼る）

並べ方は「コード → kind の辞書順」。variant は kind を `_` で区切って各語の先頭を大文字にした名前です（`use_after_move` は `UseAfterMove`）。

```rust
diagnostic_kinds! {
    since "0.1.0" {
        // E0001
        (BigintSuffixWithFraction, "bigint_suffix_with_fraction", "E0001"),
        (ByteLiteralNotAscii, "byte_literal_not_ascii", "E0001"),
        (ByteSuffixOnInterpolatedString, "byte_suffix_on_interpolated_string", "E0001"),
        (ByteSuffixOnUnsupportedLiteral, "byte_suffix_on_unsupported_literal", "E0001"),
        (CharLiteralNotOneUnit, "char_literal_not_one_unit", "E0001"),
        (CommentInInterpolationHole, "comment_in_interpolation_hole", "E0001"),
        (ExpectedDigit, "expected_digit", "E0001"),
        (FloatSuffixOnHexOrBinaryLiteral, "float_suffix_on_hex_or_binary_literal", "E0001"),
        (FormatFieldOutOfRange, "format_field_out_of_range", "E0001"),
        (FormatPrecisionOnInteger, "format_precision_on_integer", "E0001"),
        (FormatTypeNeedsPrecision, "format_type_needs_precision", "E0001"),
        (InterpolationHoleSpansLines, "interpolation_hole_spans_lines", "E0001"),
        (InvalidFormatSpec, "invalid_format_spec", "E0001"),
        (InvalidNumericLiteral, "invalid_numeric_literal", "E0001"),
        (InvalidStringEscape, "invalid_string_escape", "E0001"),
        (InvalidTypeVariableStart, "invalid_type_variable_start", "E0001"),
        (InvalidUnicodeScalarEscape, "invalid_unicode_scalar_escape", "E0001"),
        (NumericSeparatorMisplaced, "numeric_separator_misplaced", "E0001"),
        (RawNewlineInString, "raw_newline_in_string", "E0001"),
        (SingleClosingBraceInInterpolation, "single_closing_brace_in_interpolation", "E0001"),
        (UnexpectedCharacter, "unexpected_character", "E0001"),
        (UnterminatedBlockComment, "unterminated_block_comment", "E0001"),
        (UnterminatedInterpolationHole, "unterminated_interpolation_hole", "E0001"),
        (UnterminatedStringLiteral, "unterminated_string_literal", "E0001"),
        (Utf8charNotOneScalar, "utf8char_not_one_scalar", "E0001"),
        // E0002
        (AliasNameOnSameLine, "alias_name_on_same_line", "E0002"),
        (AngleBracketSpacing, "angle_bracket_spacing", "E0002"),
        (BuilderAliasIsContextualKeyword, "builder_alias_is_contextual_keyword", "E0002"),
        (BuilderAliasNameCase, "builder_alias_name_case", "E0002"),
        (CheckedLetRec, "checked_let_rec", "E0002"),
        (ClassNeedsOneTypeParameter, "class_needs_one_type_parameter", "E0002"),
        (ComputationMatchWithoutArms, "computation_match_without_arms", "E0002"),
        (ComputationNestingTooDeep, "computation_nesting_too_deep", "E0002"),
        (ConstExportOrder, "const_export_order", "E0002"),
        (ConstraintLineIndentation, "constraint_line_indentation", "E0002"),
        (CpuAttributeOnConstDef, "cpu_attribute_on_const_def", "E0002"),
        (DefinitionWithoutSignature, "definition_without_signature", "E0002"),
        (DoBangOutsideTask, "do_bang_outside_task", "E0002"),
        (DuplicateBuilderAlias, "duplicate_builder_alias", "E0002"),
        (DuplicateCpuTarget, "duplicate_cpu_target", "E0002"),
        (DuplicateJsonAttribute, "duplicate_json_attribute", "E0002"),
        (DuplicateManifestKey, "duplicate_manifest_key", "E0002"),
        (DynWithTypeArguments, "dyn_with_type_arguments", "E0002"),
        (EmptyInterpolationHole, "empty_interpolation_hole", "E0002"),
        (EmptyRegionList, "empty_region_list", "E0002"),
        (ExpectedBindingEnd, "expected_binding_end", "E0002"),
        (ExpectedBodyExpression, "expected_body_expression", "E0002"),
        (ExpectedBuilderAlias, "expected_builder_alias", "E0002"),
        (ExpectedComputationBody, "expected_computation_body", "E0002"),
        (ExpectedCpuTarget, "expected_cpu_target", "E0002"),
        (ExpectedDynClass, "expected_dyn_class", "E0002"),
        (ExpectedExpression, "expected_expression", "E0002"),
        (ExpectedExpressionAfterAttribute, "expected_expression_after_attribute", "E0002"),
        (ExpectedIdentifier, "expected_identifier", "E0002"),
        (ExpectedInterpolationClose, "expected_interpolation_close", "E0002"),
        (ExpectedKind, "expected_kind", "E0002"),
        (ExpectedLineBreak, "expected_line_break", "E0002"),
        (ExpectedLinkNameString, "expected_link_name_string", "E0002"),
        (ExpectedPattern, "expected_pattern", "E0002"),
        (ExpectedSingleTypeArgument, "expected_single_type_argument", "E0002"),
        (ExpectedToken, "expected_token", "E0002"),
        (ExpectedTypeClose, "expected_type_close", "E0002"),
        (ExpectedUnionCaseName, "expected_union_case_name", "E0002"),
        (ExpectedWhereBindings, "expected_where_bindings", "E0002"),
        (ExportOnImplementation, "export_on_implementation", "E0002"),
        (ExpressionNestingTooDeep, "expression_nesting_too_deep", "E0002"),
        (ExternTypeWithBody, "extern_type_with_body", "E0002"),
        (ExternTypeWithLinkName, "extern_type_with_link_name", "E0002"),
        (ForLoopNeedsIdentifier, "for_loop_needs_identifier", "E0002"),
        (HeaderPathOnSameLine, "header_path_on_same_line", "E0002"),
        (HeaderPathSpacing, "header_path_spacing", "E0002"),
        (IntegerLiteralTooLarge, "integer_literal_too_large", "E0002"),
        (InvalidDependencyForm, "invalid_dependency_form", "E0002"),
        (InvalidDependencyNativeFlag, "invalid_dependency_native_flag", "E0002"),
        (InvalidDependencyPath, "invalid_dependency_path", "E0002"),
        (InvalidFixedArrayLength, "invalid_fixed_array_length", "E0002"),
        (InvalidGitDependency, "invalid_git_dependency", "E0002"),
        (InvalidJsonAttribute, "invalid_json_attribute", "E0002"),
        (InvalidLinkNameText, "invalid_link_name_text", "E0002"),
        (InvalidNativeLibraryName, "invalid_native_library_name", "E0002"),
        (InvalidNativePath, "invalid_native_path", "E0002"),
        (InvalidNativeSection, "invalid_native_section", "E0002"),
        (InvalidPackageField, "invalid_package_field", "E0002"),
        (InvalidRegistryDependency, "invalid_registry_dependency", "E0002"),
        (InvalidRegistrySetting, "invalid_registry_setting", "E0002"),
        (InvalidWasmSize, "invalid_wasm_size", "E0002"),
        (LambdaWithoutParameters, "lambda_without_parameters", "E0002"),
        (LengthParameterType, "length_parameter_type", "E0002"),
        (LetFunctionNeedsLambda, "let_function_needs_lambda", "E0002"),
        (LetRecNeedsLambda, "let_rec_needs_lambda", "E0002"),
        (ListTypeWithLength, "list_type_with_length", "E0002"),
        (MalformedConstraintLine, "malformed_constraint_line", "E0002"),
        (MalformedMultiCaseActivePattern, "malformed_multi_case_active_pattern", "E0002"),
        (MalformedNewCollection, "malformed_new_collection", "E0002"),
        (ManifestSectionOrder, "manifest_section_order", "E0002"),
        (ManifestSyntaxError, "manifest_syntax_error", "E0002"),
        (MatchWithoutArms, "match_without_arms", "E0002"),
        (MisplacedCheckedAttribute, "misplaced_checked_attribute", "E0002"),
        (MisplacedConstModifier, "misplaced_const_modifier", "E0002"),
        (MisplacedCpuAttribute, "misplaced_cpu_attribute", "E0002"),
        (MisplacedDocComment, "misplaced_doc_comment", "E0002"),
        (MisplacedJsonAttribute, "misplaced_json_attribute", "E0002"),
        (MisplacedNamespaceDeclaration, "misplaced_namespace_declaration", "E0002"),
        (MisplacedRegionList, "misplaced_region_list", "E0002"),
        (MisplacedUsingDeclaration, "misplaced_using_declaration", "E0002"),
        (MissingPackageField, "missing_package_field", "E0002"),
        (MissingRegistryIndex, "missing_registry_index", "E0002"),
        (ModuleDeclarationUnsupported, "module_declaration_unsupported", "E0002"),
        (MutAsExpression, "mut_as_expression", "E0002"),
        (MutableLetRec, "mutable_let_rec", "E0002"),
        (NegativePatternNeedsNumber, "negative_pattern_needs_number", "E0002"),
        (PatternNestingTooDeep, "pattern_nesting_too_deep", "E0002"),
        (PrefixKeywordOperand, "prefix_keyword_operand", "E0002"),
        (RecursiveLetBang, "recursive_let_bang", "E0002"),
        (RecursiveWhereBinding, "recursive_where_binding", "E0002"),
        (ReturnNotLastInComputation, "return_not_last_in_computation", "E0002"),
        (ReturnOutsideTaskEnd, "return_outside_task_end", "E0002"),
        (SignatureUsesDef, "signature_uses_def", "E0002"),
        (SignatureUsesDoubleColon, "signature_uses_double_colon", "E0002"),
        (SignatureWithoutDefinition, "signature_without_definition", "E0002"),
        (SliceMustBeBorrowed, "slice_must_be_borrowed", "E0002"),
        (SliceWithoutBounds, "slice_without_bounds", "E0002"),
        (StrayAndMember, "stray_and_member", "E0002"),
        (SyntaxNestingTooDeep, "syntax_nesting_too_deep", "E0002"),
        (TooManyActivePatternCases, "too_many_active_pattern_cases", "E0002"),
        (TooManyDynClasses, "too_many_dyn_classes", "E0002"),
        (TooManyExternLinkNames, "too_many_extern_link_names", "E0002"),
        (TooManyFunctionParameters, "too_many_function_parameters", "E0002"),
        (TooManyInterpolationHoles, "too_many_interpolation_holes", "E0002"),
        (TooManyLambdaParameters, "too_many_lambda_parameters", "E0002"),
        (TooManyTupleElements, "too_many_tuple_elements", "E0002"),
        (TooManyTypeArguments, "too_many_type_arguments", "E0002"),
        (TypeParametersNeedAngleBrackets, "type_parameters_need_angle_brackets", "E0002"),
        (TypedAndNeedsBody, "typed_and_needs_body", "E0002"),
        (UnknownCpuTarget, "unknown_cpu_target", "E0002"),
        (UnknownManifestKey, "unknown_manifest_key", "E0002"),
        (UnsupportedEdition, "unsupported_edition", "E0002"),
        (UseBangInAndGroup, "use_bang_in_and_group", "E0002"),
        (UseBangOutsideComputation, "use_bang_outside_computation", "E0002"),
        (WhereBindingIndentation, "where_binding_indentation", "E0002"),
        // E0003
        (SourceTooLarge, "source_too_large", "E0003"),
        // E1001
        (DuplicateAndBinding, "duplicate_and_binding", "E1001"),
        (DuplicateDerivedClass, "duplicate_derived_class", "E1001"),
        (DuplicateExportName, "duplicate_export_name", "E1001"),
        (DuplicateExternDeclaration, "duplicate_extern_declaration", "E1001"),
        (DuplicateFunctionDefinition, "duplicate_function_definition", "E1001"),
        (DuplicateFunctionSignature, "duplicate_function_signature", "E1001"),
        (DuplicateName, "duplicate_name", "E1001"),
        // E1002
        (ClassHasNoMethod, "class_has_no_method", "E1002"),
        (UnknownFunction, "unknown_function", "E1002"),
        (UnknownLengthConstant, "unknown_length_constant", "E1002"),
        (UnknownModuleMember, "unknown_module_member", "E1002"),
        (UnknownUnionCase, "unknown_union_case", "E1002"),
        (UnknownValue, "unknown_value", "E1002"),
        // E1003
        (ActiveRecognizerResult, "active_recognizer_result", "E1003"),
        (FixedArrayLiteralLength, "fixed_array_literal_length", "E1003"),
        (FormatSpecTypeMismatch, "format_spec_type_mismatch", "E1003"),
        (FormatSpecTypeUnknown, "format_spec_type_unknown", "E1003"),
        (TypeMismatch, "type_mismatch", "E1003"),
        // E1004
        (AmbiguousBuilderAlias, "ambiguous_builder_alias", "E1004"),
        (AmbiguousName, "ambiguous_name", "E1004"),
        (AmbiguousPatternName, "ambiguous_pattern_name", "E1004"),
        (DynNeedsTypeClass, "dyn_needs_type_class", "E1004"),
        (EmptyListNeedsAnnotation, "empty_list_needs_annotation", "E1004"),
        (ExpectedLength, "expected_length", "E1004"),
        (LengthUsedAsType, "length_used_as_type", "E1004"),
        (NotARecordType, "not_a_record_type", "E1004"),
        (RepeatedModuleName, "repeated_module_name", "E1004"),
        (StdItemMissing, "std_item_missing", "E1004"),
        (TypeArgumentCount, "type_argument_count", "E1004"),
        (TypeUsedAsLength, "type_used_as_length", "E1004"),
        (UnknownTypeName, "unknown_type_name", "E1004"),
        // E1005
        (AtomicElementType, "atomic_element_type", "E1005"),
        (CallNonFunction, "call_non_function", "E1005"),
        (CaptureExclusiveInReusableFunction, "capture_exclusive_in_reusable_function", "E1005"),
        (CaptureRcInFunctionValue, "capture_rc_in_function_value", "E1005"),
        (CaptureSyncPrimitiveInFunctionValue, "capture_sync_primitive_in_function_value", "E1005"),
        (CaptureUnsendableInFunctionValue, "capture_unsendable_in_function_value", "E1005"),
        (CollectionElementExclusiveBorrow, "collection_element_exclusive_borrow", "E1005"),
        (CpuFunctionWideVectorBoundary, "cpu_function_wide_vector_boundary", "E1005"),
        (DerefNonReference, "deref_non_reference", "E1005"),
        (ExclusiveSliceWholeArrayParameter, "exclusive_slice_whole_array_parameter", "E1005"),
        (FixedArrayInitAnnotation, "fixed_array_init_annotation", "E1005"),
        (FixedSliceSource, "fixed_slice_source", "E1005"),
        (ForOverFixedArray, "for_over_fixed_array", "E1005"),
        (ForSourceNotIterable, "for_source_not_iterable", "E1005"),
        (FunctionConstraintUnresolvable, "function_constraint_unresolvable", "E1005"),
        (FunctionConstraintUnsatisfied, "function_constraint_unsatisfied", "E1005"),
        (IndexNonIndexable, "index_non_indexable", "E1005"),
        (InvalidLengthConstant, "invalid_length_constant", "E1005"),
        (NewOnFixedArray, "new_on_fixed_array", "E1005"),
        (NoInstance, "no_instance", "E1005"),
        (NoInstanceForOperator, "no_instance_for_operator", "E1005"),
        (NoWiderInteger, "no_wider_integer", "E1005"),
        (RangeAsValue, "range_as_value", "E1005"),
        (RecFunctionCapturesGeneric, "rec_function_captures_generic", "E1005"),
        (RecordUpdateNonRecord, "record_update_non_record", "E1005"),
        (SharedContainerExclusiveBorrow, "shared_container_exclusive_borrow", "E1005"),
        (SimdLaneCountMismatch, "simd_lane_count_mismatch", "E1005"),
        (SimdOperationVectorType, "simd_operation_vector_type", "E1005"),
        (SliceNonArray, "slice_non_array", "E1005"),
        (SliceOfFixedArray, "slice_of_fixed_array", "E1005"),
        (StdRepresentationMismatch, "std_representation_mismatch", "E1005"),
        (UnsizedSliceTypePosition, "unsized_slice_type_position", "E1005"),
        (UseRequiresDrop, "use_requires_drop", "E1005"),
        // E1006
        (ActivePatternArgumentCount, "active_pattern_argument_count", "E1006"),
        (ActiveRecognizerParameters, "active_recognizer_parameters", "E1006"),
        (DefinitionParameterCountMismatch, "definition_parameter_count_mismatch", "E1006"),
        (InstanceMethodParameterCount, "instance_method_parameter_count", "E1006"),
        (OwnedFunctionParameterCount, "owned_function_parameter_count", "E1006"),
        (TooManyArgumentsApplied, "too_many_arguments_applied", "E1006"),
        // E1007
        (FieldAccessNonRecord, "field_access_non_record", "E1007"),
        (MissingRecordFields, "missing_record_fields", "E1007"),
        (RecordHasNoField, "record_has_no_field", "E1007"),
        (UnknownField, "unknown_field", "E1007"),
        // E1008
        (ConflictingExternDeclaration, "conflicting_extern_declaration", "E1008"),
        (ExportActiveRecognizer, "export_active_recognizer", "E1008"),
        (ExportOnExternDeclaration, "export_on_extern_declaration", "E1008"),
        (ExternCallbackArgument, "extern_callback_argument", "E1008"),
        (ExternCallbackCallForm, "extern_callback_call_form", "E1008"),
        (ExternDeclarationConstraints, "extern_declaration_constraints", "E1008"),
        (InvalidExternSymbol, "invalid_extern_symbol", "E1008"),
        (InvalidWasmImportModule, "invalid_wasm_import_module", "E1008"),
        (ReservedExternSymbol, "reserved_extern_symbol", "E1008"),
        (UnsupportedExportSignature, "unsupported_export_signature", "E1008"),
        (UnsupportedExternSignature, "unsupported_extern_signature", "E1008"),
        // E1009
        (DecimalLiteralOutOfRange, "decimal_literal_out_of_range", "E1009"),
        (FloatLiteralOutOfRange, "float_literal_out_of_range", "E1009"),
        (FloatLiteralType, "float_literal_type", "E1009"),
        (IntegerLiteralOutOfRange, "integer_literal_out_of_range", "E1009"),
        (IntegerLiteralType, "integer_literal_type", "E1009"),
        (InvalidBigintDigit, "invalid_bigint_digit", "E1009"),
        (InvalidFloatLiteral, "invalid_float_literal", "E1009"),
        // E1010
        (FixedArrayTooLong, "fixed_array_too_long", "E1010"),
        (InfiniteRecursiveLayout, "infinite_recursive_layout", "E1010"),
        (LayoutNestingLimit, "layout_nesting_limit", "E1010"),
        (RecursiveUnionWithoutBaseCase, "recursive_union_without_base_case", "E1010"),
        (ValueLayoutTooLarge, "value_layout_too_large", "E1010"),
        // E1011
        (AmbiguousPackageRoot, "ambiguous_package_root", "E1011"),
        (ConflictingDependencySource, "conflicting_dependency_source", "E1011"),
        (CyclicPackageDependency, "cyclic_package_dependency", "E1011"),
        (DependencyKindNotAllowed, "dependency_kind_not_allowed", "E1011"),
        (DependencyNameMismatch, "dependency_name_mismatch", "E1011"),
        (DependencyPathNotRelative, "dependency_path_not_relative", "E1011"),
        (DuplicateDependency, "duplicate_dependency", "E1011"),
        (DuplicateModule, "duplicate_module", "E1011"),
        (IncompatibleDependencyVersions, "incompatible_dependency_versions", "E1011"),
        (InvalidModuleFilename, "invalid_module_filename", "E1011"),
        (InvalidModuleName, "invalid_module_name", "E1011"),
        (InvalidPackageName, "invalid_package_name", "E1011"),
        (InvalidPackageNamespace, "invalid_package_namespace", "E1011"),
        (InvalidPublishVersion, "invalid_publish_version", "E1011"),
        (InvalidSourcePath, "invalid_source_path", "E1011"),
        (InvalidStdPath, "invalid_std_path", "E1011"),
        (InvalidUsingDeclaration, "invalid_using_declaration", "E1011"),
        (ManifestNotRegularFile, "manifest_not_regular_file", "E1011"),
        (ModuleNamespaceCollision, "module_namespace_collision", "E1011"),
        (ModuleSourceNotRegularFile, "module_source_not_regular_file", "E1011"),
        (PublishNonRegistryDependency, "publish_non_registry_dependency", "E1011"),
        (ReservedPackageNamespace, "reserved_package_namespace", "E1011"),
        (ReservedStdNamespace, "reserved_std_namespace", "E1011"),
        (SourceFilenameCaseMismatch, "source_filename_case_mismatch", "E1011"),
        (SourceOutsidePackageRoot, "source_outside_package_root", "E1011"),
        (SourceWithoutFilename, "source_without_filename", "E1011"),
        (SymlinkNotFollowed, "symlink_not_followed", "E1011"),
        // E1012
        (AssignToImmutablePlace, "assign_to_immutable_place", "E1012"),
        (DropPassExclusiveValue, "drop_pass_exclusive_value", "E1012"),
        (DropReplaceWholeValue, "drop_replace_whole_value", "E1012"),
        (MoveOutOfCollectionElement, "move_out_of_collection_element", "E1012"),
        (MoveOutOfDropType, "move_out_of_drop_type", "E1012"),
        (MoveOutOfOwnedFunction, "move_out_of_owned_function", "E1012"),
        (MoveOutOfRecFunction, "move_out_of_rec_function", "E1012"),
        (MoveOutOfReferenceBinding, "move_out_of_reference_binding", "E1012"),
        (UpdateDropTypeValue, "update_drop_type_value", "E1012"),
        (UseAfterMove, "use_after_move", "E1012"),
        // E1013
        (AssignmentExtendsBorrow, "assignment_extends_borrow", "E1013"),
        (BorrowAcrossAsyncBoundary, "borrow_across_async_boundary", "E1013"),
        (BorrowFieldWithoutRegion, "borrow_field_without_region", "E1013"),
        (BorrowOfTemporary, "borrow_of_temporary", "E1013"),
        (BorrowOutlivesBinding, "borrow_outlives_binding", "E1013"),
        (BorrowOutlivesBlock, "borrow_outlives_block", "E1013"),
        (BorrowedUnionPayload, "borrowed_union_payload", "E1013"),
        (CallbackContractUnsatisfied, "callback_contract_unsatisfied", "E1013"),
        (ChannelItemHoldsBorrow, "channel_item_holds_borrow", "E1013"),
        (ConstTemporaryBorrowEscapes, "const_temporary_borrow_escapes", "E1013"),
        (DynHoldsBorrowWithoutRegion, "dyn_holds_borrow_without_region", "E1013"),
        (DynHoldsExclusiveBorrow, "dyn_holds_exclusive_borrow", "E1013"),
        (ExclusiveBorrowBehindRegion, "exclusive_borrow_behind_region", "E1013"),
        (ExclusiveBorrowFieldShape, "exclusive_borrow_field_shape", "E1013"),
        (ExclusiveFieldRegionsMissing, "exclusive_field_regions_missing", "E1013"),
        (ExclusiveFieldWithoutRegion, "exclusive_field_without_region", "E1013"),
        (ExclusiveRegionReused, "exclusive_region_reused", "E1013"),
        (FieldMixesRegions, "field_mixes_regions", "E1013"),
        (InvalidRegionNames, "invalid_region_names", "E1013"),
        (MisplacedRegionQuantifier, "misplaced_region_quantifier", "E1013"),
        (MultipleRegionsOnValue, "multiple_regions_on_value", "E1013"),
        (MustApplyDirectly, "must_apply_directly", "E1013"),
        (MutableRecordField, "mutable_record_field", "E1013"),
        (MutableRegionQuantifiedParameter, "mutable_region_quantified_parameter", "E1013"),
        (MutexResultRetainsBorrow, "mutex_result_retains_borrow", "E1013"),
        (NamedRegionInFunctionType, "named_region_in_function_type", "E1013"),
        (NamedRegionInLocal, "named_region_in_local", "E1013"),
        (NestedBorrowUnsupported, "nested_borrow_unsupported", "E1013"),
        (OwnedFunctionCapturesReference, "owned_function_captures_reference", "E1013"),
        (ParallelCallbackBorrowsEnvironment, "parallel_callback_borrows_environment", "E1013"),
        (ParallelInputBorrowsEnvironment, "parallel_input_borrows_environment", "E1013"),
        (ParallelResultRetainsBorrow, "parallel_result_retains_borrow", "E1013"),
        (ReferenceRegionCount, "reference_region_count", "E1013"),
        (ReferenceWithoutOwner, "reference_without_owner", "E1013"),
        (RegionAlreadyDeclared, "region_already_declared", "E1013"),
        (RegionBelongsToFunction, "region_belongs_to_function", "E1013"),
        (RegionCallbackStoresExclusiveInput, "region_callback_stores_exclusive_input", "E1013"),
        (RegionCountMismatch, "region_count_mismatch", "E1013"),
        (RegionOnNonBorrow, "region_on_non_borrow", "E1013"),
        (RegionQuantifierTarget, "region_quantifier_target", "E1013"),
        (ResultRegionMismatch, "result_region_mismatch", "E1013"),
        (ResultRegionMissing, "result_region_missing", "E1013"),
        (ResultRegionWithoutInput, "result_region_without_input", "E1013"),
        (SelfReferentialStore, "self_referential_store", "E1013"),
        (StoreLocalBorrowThroughReference, "store_local_borrow_through_reference", "E1013"),
        (StoreRegionMismatch, "store_region_mismatch", "E1013"),
        (StoreRegionUnnamed, "store_region_unnamed", "E1013"),
        (TaskCaptureRetainsBorrow, "task_capture_retains_borrow", "E1013"),
        (TaskRequiresOwnedValue, "task_requires_owned_value", "E1013"),
        (TaskRequiresSend, "task_requires_send", "E1013"),
        (TaskRequiresSync, "task_requires_sync", "E1013"),
        (TaskResultIsReference, "task_result_is_reference", "E1013"),
        (TaskResultRetainsBorrow, "task_result_retains_borrow", "E1013"),
        (TaskScopeSharesBorrowedEnvironment, "task_scope_shares_borrowed_environment", "E1013"),
        (UndeclaredRegion, "undeclared_region", "E1013"),
        (UnionPayloadExclusiveBorrow, "union_payload_exclusive_borrow", "E1013"),
        (UnusedRegion, "unused_region", "E1013"),
        // E1014
        (ActivePatternMutatesInput, "active_pattern_mutates_input", "E1014"),
        (BorrowConflict, "borrow_conflict", "E1014"),
        (ExclusiveSliceSource, "exclusive_slice_source", "E1014"),
        (MoveReborrowedExclusiveReference, "move_reborrowed_exclusive_reference", "E1014"),
        (MutateImmutableValue, "mutate_immutable_value", "E1014"),
        (MutateThroughSharedReference, "mutate_through_shared_reference", "E1014"),
        (ReplaceExclusiveSlice, "replace_exclusive_slice", "E1014"),
        // E1015
        (AmbiguousPolymorphicUse, "ambiguous_polymorphic_use", "E1015"),
        (ConcreteIntegerRequired, "concrete_integer_required", "E1015"),
        (ConcreteTypeRequired, "concrete_type_required", "E1015"),
        (ConstructorKindMismatch, "constructor_kind_mismatch", "E1015"),
        (ConstructorVariableKindAnnotation, "constructor_variable_kind_annotation", "E1015"),
        (DynDispatchResultType, "dyn_dispatch_result_type", "E1015"),
        (DynOfArity, "dyn_of_arity", "E1015"),
        (DynOfNeedsExpectedType, "dyn_of_needs_expected_type", "E1015"),
        (ExpectedTypeConstructor, "expected_type_constructor", "E1015"),
        (ExternSignatureNotConcrete, "extern_signature_not_concrete", "E1015"),
        (ExternTypeAsConstructor, "extern_type_as_constructor", "E1015"),
        (FixedArrayLengthUnknown, "fixed_array_length_unknown", "E1015"),
        (HigherKindedAlias, "higher_kinded_alias", "E1015"),
        (HigherOrderKindArgument, "higher_order_kind_argument", "E1015"),
        (InfiniteInferredType, "infinite_inferred_type", "E1015"),
        (InstanceHeadNotConstructor, "instance_head_not_constructor", "E1015"),
        (InternalRecBindingNotLambda, "internal_rec_binding_not_lambda", "E1015"),
        (InternalRecGroupUnbound, "internal_rec_group_unbound", "E1015"),
        (InternalRecNotBoundByBlock, "internal_rec_not_bound_by_block", "E1015"),
        (InternalUnlowerableIr, "internal_unlowerable_ir", "E1015"),
        (MissingTypeArguments, "missing_type_arguments", "E1015"),
        (PolymorphicEntryPoint, "polymorphic_entry_point", "E1015"),
        (RefOperandInferredLate, "ref_operand_inferred_late", "E1015"),
        (ScalarAsTypeConstructor, "scalar_as_type_constructor", "E1015"),
        (SimdVectorTypeUnknown, "simd_vector_type_unknown", "E1015"),
        (SuperclassKindMismatch, "superclass_kind_mismatch", "E1015"),
        (TypeConstructorNotApplied, "type_constructor_not_applied", "E1015"),
        (TypeVariableAbsentFromSignature, "type_variable_absent_from_signature", "E1015"),
        (TypeVariableKindConflict, "type_variable_kind_conflict", "E1015"),
        // E1016
        (BuiltinInstanceOverride, "builtin_instance_override", "E1016"),
        (ClassConstraintArity, "class_constraint_arity", "E1016"),
        (ClassMethodConstraint, "class_method_constraint", "E1016"),
        (ClassMethodTypeVariable, "class_method_type_variable", "E1016"),
        (ClassWithoutMethods, "class_without_methods", "E1016"),
        (DefaultBodyUnknownMethod, "default_body_unknown_method", "E1016"),
        (DropInstanceConstraint, "drop_instance_constraint", "E1016"),
        (DropInstanceGeneric, "drop_instance_generic", "E1016"),
        (DropInstanceTarget, "drop_instance_target", "E1016"),
        (ExplicitDropCall, "explicit_drop_call", "E1016"),
        (FieldTypeConstraint, "field_type_constraint", "E1016"),
        (FormatInstanceTarget, "format_instance_target", "E1016"),
        (InstanceMissingMethod, "instance_missing_method", "E1016"),
        (InstanceUnknownMethod, "instance_unknown_method", "E1016"),
        (MissingExplicitConstraint, "missing_explicit_constraint", "E1016"),
        (OverlappingInstance, "overlapping_instance", "E1016"),
        (UnknownTypeClass, "unknown_type_class", "E1016"),
        // E1017
        (AndGroupTooLarge, "and_group_too_large", "E1017"),
        (ClassConstraintLimit, "class_constraint_limit", "E1017"),
        (DerivingExpressionLimit, "deriving_expression_limit", "E1017"),
        (DerivingInstanceLimit, "deriving_instance_limit", "E1017"),
        (ExhaustivenessSearchLimit, "exhaustiveness_search_limit", "E1017"),
        (FunctionConstraintLimit, "function_constraint_limit", "E1017"),
        (GitDependencyTooLarge, "git_dependency_too_large", "E1017"),
        (GpuKernelLimit, "gpu_kernel_limit", "E1017"),
        (HeaderPathTooLong, "header_path_too_long", "E1017"),
        (InstanceOverlapLimit, "instance_overlap_limit", "E1017"),
        (InstanceResolutionCycle, "instance_resolution_cycle", "E1017"),
        (InstanceResolutionLimit, "instance_resolution_limit", "E1017"),
        (LockfileTooLarge, "lockfile_too_large", "E1017"),
        (LoopOwnershipLimit, "loop_ownership_limit", "E1017"),
        (ManifestTooLarge, "manifest_too_large", "E1017"),
        (ModuleDiscoveryLimit, "module_discovery_limit", "E1017"),
        (ModulePathLimit, "module_path_limit", "E1017"),
        (NamespacePathTooLong, "namespace_path_too_long", "E1017"),
        (OrPatternAlternativesLimit, "or_pattern_alternatives_limit", "E1017"),
        (PackageGraphLimit, "package_graph_limit", "E1017"),
        (PatternAlternativesLimit, "pattern_alternatives_limit", "E1017"),
        (PolymorphicConstraintLimit, "polymorphic_constraint_limit", "E1017"),
        (PolymorphicRecursionGrowth, "polymorphic_recursion_growth", "E1017"),
        (PolymorphicTypeExpansionLimit, "polymorphic_type_expansion_limit", "E1017"),
        (QualifiedNameTooLong, "qualified_name_too_long", "E1017"),
        (RecursiveTypeChangesArguments, "recursive_type_changes_arguments", "E1017"),
        (RecursiveTypeExpansionLimit, "recursive_type_expansion_limit", "E1017"),
        (RegistryIndexTooLarge, "registry_index_too_large", "E1017"),
        (RegistryResolutionLimit, "registry_resolution_limit", "E1017"),
        (SequenceLoopDepthLimit, "sequence_loop_depth_limit", "E1017"),
        (SpecializationLimit, "specialization_limit", "E1017"),
        (SuperclassResolutionLimit, "superclass_resolution_limit", "E1017"),
        (TooManyDependencies, "too_many_dependencies", "E1017"),
        (TooManyLoopExits, "too_many_loop_exits", "E1017"),
        (TooManyRaisePoints, "too_many_raise_points", "E1017"),
        (TooManyRecordRegions, "too_many_record_regions", "E1017"),
        (TooManyRegionParameters, "too_many_region_parameters", "E1017"),
        (TooManyTrapLocations, "too_many_trap_locations", "E1017"),
        (TypeAliasExpansionLimit, "type_alias_expansion_limit", "E1017"),
        // E1018
        (AliasOutsideBuilderFile, "alias_outside_builder_file", "E1018"),
        (BenchClockOutsideBench, "bench_clock_outside_bench", "E1018"),
        (BuilderMissingOperation, "builder_missing_operation", "E1018"),
        (BuilderWithoutOperations, "builder_without_operations", "E1018"),
        (CannotInferBuilder, "cannot_infer_builder", "E1018"),
        (ClassOutsideClassFile, "class_outside_class_file", "E1018"),
        (ComputationWithoutBuilder, "computation_without_builder", "E1018"),
        (DeclarationInClassFile, "declaration_in_class_file", "E1018"),
        (GpuBufferElementType, "gpu_buffer_element_type", "E1018"),
        (GpuCallbackNotKnown, "gpu_callback_not_known", "E1018"),
        (GpuKernelMissing, "gpu_kernel_missing", "E1018"),
        (GpuKernelRecursion, "gpu_kernel_recursion", "E1018"),
        (GpuKernelSignature, "gpu_kernel_signature", "E1018"),
        (GpuOperationNotDirect, "gpu_operation_not_direct", "E1018"),
        (GpuUnsupportedConstruct, "gpu_unsupported_construct", "E1018"),
        (GpuUnsupportedOperator, "gpu_unsupported_operator", "E1018"),
        (GpuUnsupportedType, "gpu_unsupported_type", "E1018"),
        (ImplicitTaskNeedsBuilder, "implicit_task_needs_builder", "E1018"),
        (NoBuilderForValue, "no_builder_for_value", "E1018"),
        (StdDeclaresTest, "std_declares_test", "E1018"),
        (StdExportsFunction, "std_exports_function", "E1018"),
        (UnknownComputationBuilder, "unknown_computation_builder", "E1018"),
        // E1019
        (AndWithoutRecursiveGroup, "and_without_recursive_group", "E1019"),
        (MismatchedRecursiveGroup, "mismatched_recursive_group", "E1019"),
        (MissingRecMarker, "missing_rec_marker", "E1019"),
        // E1020
        (ActiveArgumentNotValue, "active_argument_not_value", "E1020"),
        (ActivePatternCaseCount, "active_pattern_case_count", "E1020"),
        (ActivePatternCaseName, "active_pattern_case_name", "E1020"),
        (ArrayPatternOnFixedArray, "array_pattern_on_fixed_array", "E1020"),
        (CasePatternMissingPayload, "case_pattern_missing_payload", "E1020"),
        (CasePatternPayloadShape, "case_pattern_payload_shape", "E1020"),
        (CasePatternUnexpectedPayload, "case_pattern_unexpected_payload", "E1020"),
        (DuplicateActivePatternCase, "duplicate_active_pattern_case", "E1020"),
        (ExpressionInPattern, "expression_in_pattern", "E1020"),
        (MultiCaseActivePatternResultType, "multi_case_active_pattern_result_type", "E1020"),
        (NullPattern, "null_pattern", "E1020"),
        (OrPatternBindingMismatch, "or_pattern_binding_mismatch", "E1020"),
        (PartialActivePatternCaseCount, "partial_active_pattern_case_count", "E1020"),
        (QualifiedPatternBinding, "qualified_pattern_binding", "E1020"),
        (RecordPatternUnknownType, "record_pattern_unknown_type", "E1020"),
        (ReservedActiveResultVariable, "reserved_active_result_variable", "E1020"),
        (UnknownCaseOrActivePattern, "unknown_case_or_active_pattern", "E1020"),
        // E1021
        (NonExhaustiveMatch, "non_exhaustive_match", "E1021"),
        // E1022
        (OpaqueTypeRepresentation, "opaque_type_representation", "E1022"),
        (PrivateBuilderOperation, "private_builder_operation", "E1022"),
        (PrivateExport, "private_export", "E1022"),
        (PrivateNameAccess, "private_name_access", "E1022"),
        (PrivateOnUnsupportedDeclaration, "private_on_unsupported_declaration", "E1022"),
        (PrivateStdPrimitive, "private_std_primitive", "E1022"),
        (PrivateTypeInPublicApi, "private_type_in_public_api", "E1022"),
        // E1023
        (JumpOutsideLoop, "jump_outside_loop", "E1023"),
        // E1024
        (AliasArgumentCount, "alias_argument_count", "E1024"),
        (CyclicTypeAlias, "cyclic_type_alias", "E1024"),
        (DuplicateTypeParameter, "duplicate_type_parameter", "E1024"),
        (DuplicateUnionCase, "duplicate_union_case", "E1024"),
        (InvalidTypeParameterName, "invalid_type_parameter_name", "E1024"),
        (TypeNameCase, "type_name_case", "E1024"),
        (UndeclaredLengthParameter, "undeclared_length_parameter", "E1024"),
        (UndeclaredTypeVariable, "undeclared_type_variable", "E1024"),
        (UnionCaseNameCase, "union_case_name_case", "E1024"),
        (UnusedTypeParameter, "unused_type_parameter", "E1024"),
        // E1025
        (DefaultDeriveRecursive, "default_derive_recursive", "E1025"),
        (DerivedInstanceInvalid, "derived_instance_invalid", "E1025"),
        (DerivingUnavailable, "deriving_unavailable", "E1025"),
        (DuplicateJsonName, "duplicate_json_name", "E1025"),
        (JsonNameWithoutDeriving, "json_name_without_deriving", "E1025"),
        (UnderivableClass, "underivable_class", "E1025"),
        // E1026
        (ConstAggregateOperator, "const_aggregate_operator", "E1026"),
        (ConstAssignmentTarget, "const_assignment_target", "E1026"),
        (ConstBorrowNotAllowed, "const_borrow_not_allowed", "E1026"),
        (ConstCallDepthLimit, "const_call_depth_limit", "E1026"),
        (ConstCalleeNotConst, "const_callee_not_const", "E1026"),
        (ConstCheckedOperation, "const_checked_operation", "E1026"),
        (ConstConditionNotBool, "const_condition_not_bool", "E1026"),
        (ConstCycle, "const_cycle", "E1026"),
        (ConstEvaluationTrapped, "const_evaluation_trapped", "E1026"),
        (ConstExpressionNotAllowed, "const_expression_not_allowed", "E1026"),
        (ConstForLoopForm, "const_for_loop_form", "E1026"),
        (ConstFunctionValue, "const_function_value", "E1026"),
        (ConstLimitExceeded, "const_limit_exceeded", "E1026"),
        (ConstNewCollectionForm, "const_new_collection_form", "E1026"),
        (ConstOperationNotAllowed, "const_operation_not_allowed", "E1026"),
        (ConstPartialApplication, "const_partial_application", "E1026"),
        (ConstPipeNotAllowed, "const_pipe_not_allowed", "E1026"),
        (ConstSignatureInvalid, "const_signature_invalid", "E1026"),
        (ConstStaticDataTooLarge, "const_static_data_too_large", "E1026"),
        (ConstUnbuildableType, "const_unbuildable_type", "E1026"),
        (ConstUnsupportedOperation, "const_unsupported_operation", "E1026"),
        (ConstantTypeNotConcrete, "constant_type_not_concrete", "E1026"),
        // E1027
        (ClassContextNotAClass, "class_context_not_a_class", "E1027"),
        (InstanceCheckFailed, "instance_check_failed", "E1027"),
        (InstanceContextVariable, "instance_context_variable", "E1027"),
        (InstanceMissingSuperclass, "instance_missing_superclass", "E1027"),
        (SuperclassCycle, "superclass_cycle", "E1027"),
        (SuperclassTypeVariable, "superclass_type_variable", "E1027"),
        (UndeclaredDefaultConstraint, "undeclared_default_constraint", "E1027"),
        (UndeclaredInstanceConstraint, "undeclared_instance_constraint", "E1027"),
        (UnknownClassInContext, "unknown_class_in_context", "E1027"),
        // E1028
        (DynIncompatibleClass, "dyn_incompatible_class", "E1028"),
        // E2000
        (BindgenAnnotationError, "bindgen_annotation_error", "E2000"),
        (BindgenArgumentError, "bindgen_argument_error", "E2000"),
        (CannotDeriveNamespace, "cannot_derive_namespace", "E2000"),
        (DebugInfoWithoutSourceMap, "debug_info_without_source_map", "E2000"),
        (DuplicateLinkInput, "duplicate_link_input", "E2000"),
        (DuplicateOption, "duplicate_option", "E2000"),
        (EnvironmentSettingInvalid, "environment_setting_invalid", "E2000"),
        (FileNotFormatted, "file_not_formatted", "E2000"),
        (ForeignTargetNotRunnable, "foreign_target_not_runnable", "E2000"),
        (FormatterChangedSemantics, "formatter_changed_semantics", "E2000"),
        (FormatterInvalidOutput, "formatter_invalid_output", "E2000"),
        (IndexOutOfRange, "index_out_of_range", "E2000"),
        (InputCountInvalid, "input_count_invalid", "E2000"),
        (InputExtensionInvalid, "input_extension_invalid", "E2000"),
        (InvalidLibraryName, "invalid_library_name", "E2000"),
        (InvalidNamespaceArgument, "invalid_namespace_argument", "E2000"),
        (InvalidPublishGitRev, "invalid_publish_git_rev", "E2000"),
        (InvalidPublishGitUrl, "invalid_publish_git_url", "E2000"),
        (InvalidRangeFlagSetting, "invalid_range_flag_setting", "E2000"),
        (InvalidThreadSetting, "invalid_thread_setting", "E2000"),
        (InvalidTrapSourceMap, "invalid_trap_source_map", "E2000"),
        (LinkInputLimit, "link_input_limit", "E2000"),
        (MissingInput, "missing_input", "E2000"),
        (NativeLinkSettingNotAllowed, "native_link_setting_not_allowed", "E2000"),
        (NetAsyncNeedsBlockOn, "net_async_needs_block_on", "E2000"),
        (NewProjectFolderNotEmpty, "new_project_folder_not_empty", "E2000"),
        (NewProjectIoFailure, "new_project_io_failure", "E2000"),
        (NoSourcesFound, "no_sources_found", "E2000"),
        (OptionConflict, "option_conflict", "E2000"),
        (OptionInvalidForCommand, "option_invalid_for_command", "E2000"),
        (OptionInvalidValue, "option_invalid_value", "E2000"),
        (OptionMissingValue, "option_missing_value", "E2000"),
        (OptionRequiresBuild, "option_requires_build", "E2000"),
        (OptionRequiresOption, "option_requires_option", "E2000"),
        (OptionUnsupportedPlatform, "option_unsupported_platform", "E2000"),
        (OutputExtensionInvalid, "output_extension_invalid", "E2000"),
        (ProjectManifestMissing, "project_manifest_missing", "E2000"),
        (ReplCommandUsage, "repl_command_usage", "E2000"),
        (ReplDeclarationNotAllowed, "repl_declaration_not_allowed", "E2000"),
        (ReplUnknownCommand, "repl_unknown_command", "E2000"),
        (ScriptWrongFileKind, "script_wrong_file_kind", "E2000"),
        (TrapInfoWithoutSourceMap, "trap_info_without_source_map", "E2000"),
        (UnknownExplainTarget, "unknown_explain_target", "E2000"),
        (UnknownOption, "unknown_option", "E2000"),
        (UnknownTarget, "unknown_target", "E2000"),
        (UsageError, "usage_error", "E2000"),
        (WasmNetNeedsOptIn, "wasm_net_needs_opt_in", "E2000"),
        (WasmOsApiNeedsOptIn, "wasm_os_api_needs_opt_in", "E2000"),
        (WasmTestRunnerAsync, "wasm_test_runner_async", "E2000"),
        // E2001
        (ForeignRunnerNotStartable, "foreign_runner_not_startable", "E2001"),
        (InputNotRegularFile, "input_not_regular_file", "E2001"),
        (InputNotUtf8, "input_not_utf8", "E2001"),
        (InputPathNotUtf8, "input_path_not_utf8", "E2001"),
        (InputTooLarge, "input_too_large", "E2001"),
        (IoFailure, "io_failure", "E2001"),
        (LinkInputUnreadable, "link_input_unreadable", "E2001"),
        (ReplStreamFailure, "repl_stream_failure", "E2001"),
        (TempDirectoryFailed, "temp_directory_failed", "E2001"),
        // E2002
        (BindgenUnreadableAst, "bindgen_unreadable_ast", "E2002"),
        (BindgenUnsupportedTarget, "bindgen_unsupported_target", "E2002"),
        (ExternalToolFailed, "external_tool_failed", "E2002"),
        (ExternalToolNotRunnable, "external_tool_not_runnable", "E2002"),
        (ExternalToolOutputTooLarge, "external_tool_output_too_large", "E2002"),
        (ExternalToolReadFailed, "external_tool_read_failed", "E2002"),
        (ExternalToolWaitFailed, "external_tool_wait_failed", "E2002"),
        (InvalidCoverageCounters, "invalid_coverage_counters", "E2002"),
        (PlatformUnsupportedApi, "platform_unsupported_api", "E2002"),
        (ToolchainMissingFeature, "toolchain_missing_feature", "E2002"),
        // E2003
        (BindgenOutputNotGenerated, "bindgen_output_not_generated", "E2003"),
        (DocFilenameCollision, "doc_filename_collision", "E2003"),
        (DocOutputChanged, "doc_output_changed", "E2003"),
        (DocOutputContainsSources, "doc_output_contains_sources", "E2003"),
        (DocOutputInvalid, "doc_output_invalid", "E2003"),
        (DocOutputNotOwned, "doc_output_not_owned", "E2003"),
        (DocPublicationFailed, "doc_publication_failed", "E2003"),
        (FormatTargetNotRegularFile, "format_target_not_regular_file", "E2003"),
        (OutputHasSourceExtension, "output_has_source_extension", "E2003"),
        (OutputNotRegularFile, "output_not_regular_file", "E2003"),
        (OutputOverwritesInput, "output_overwrites_input", "E2003"),
        (OutputOverwritesLinkInput, "output_overwrites_link_input", "E2003"),
        (OutputPublicationFailed, "output_publication_failed", "E2003"),
        (SourceChangedDuringFormatting, "source_changed_during_formatting", "E2003"),
        // E2004
        (AmbiguousEntryPoint, "ambiguous_entry_point", "E2004"),
        (BuilderFileToplevelCode, "builder_file_toplevel_code", "E2004"),
        (EntryPointMissing, "entry_point_missing", "E2004"),
        (EntryPointResultType, "entry_point_result_type", "E2004"),
        (InvalidMainSignature, "invalid_main_signature", "E2004"),
        (SpirvKernelCount, "spirv_kernel_count", "E2004"),
        (ToplevelCodeOutsideMain, "toplevel_code_outside_main", "E2004"),
        // E2005
        (BenchmarkFailed, "benchmark_failed", "E2005"),
        (ProgramExitedNonzero, "program_exited_nonzero", "E2005"),
        (ProgramTerminated, "program_terminated", "E2005"),
        (ProgramTimeLimitExceeded, "program_time_limit_exceeded", "E2005"),
        // E2006
        (TestsFailed, "tests_failed", "E2006"),
        // E2007
        (FileUrlAfterHttps, "file_url_after_https", "E2007"),
        (GitCaseCollision, "git_case_collision", "E2007"),
        (GitCommandFailed, "git_command_failed", "E2007"),
        (GitMissingManifest, "git_missing_manifest", "E2007"),
        (GitNonRegularEntry, "git_non_regular_entry", "E2007"),
        (GitSourceNotUtf8, "git_source_not_utf8", "E2007"),
        (GitTreeUnreadable, "git_tree_unreadable", "E2007"),
        (GitUnsafePath, "git_unsafe_path", "E2007"),
        (GitVersionUnsupported, "git_version_unsupported", "E2007"),
        (InvalidLockfile, "invalid_lockfile", "E2007"),
        (LockfileChecksumMismatch, "lockfile_checksum_mismatch", "E2007"),
        (LockfileEntryMissing, "lockfile_entry_missing", "E2007"),
        (LockfileMissing, "lockfile_missing", "E2007"),
        (LockfileNotRegularFile, "lockfile_not_regular_file", "E2007"),
        (LockfileVersionMismatch, "lockfile_version_mismatch", "E2007"),
        (PackageFilesUnreadable, "package_files_unreadable", "E2007"),
        (PackageNotDownloaded, "package_not_downloaded", "E2007"),
        (PackageStoreUnavailable, "package_store_unavailable", "E2007"),
        (PackageStoreUnusable, "package_store_unusable", "E2007"),
        (PublishedPackageMismatch, "published_package_mismatch", "E2007"),
        (RegistryChecksumMismatch, "registry_checksum_mismatch", "E2007"),
        (RegistryDependencyNotSelected, "registry_dependency_not_selected", "E2007"),
        (RegistryEntryMismatch, "registry_entry_mismatch", "E2007"),
        (RegistryIndexInvalid, "registry_index_invalid", "E2007"),
        (RegistryIndexMissing, "registry_index_missing", "E2007"),
        (RegistryPackageMissing, "registry_package_missing", "E2007"),
        (RegistryVersionMissing, "registry_version_missing", "E2007"),
        // W1001
        (UnusedLocal, "unused_local", "W1001"),
        // W1002
        (UnusedPrivateFunction, "unused_private_function", "W1002"),
        (UnusedPrivateType, "unused_private_type", "W1002"),
        // W1003
        (UnreachableMatchArm, "unreachable_match_arm", "W1003"),
        // W1004
        (ShadowedLocal, "shadowed_local", "W1004"),
        // W1005
        (DeprecatedUse, "deprecated_use", "W1005"),
        // W1006
        (ImplicitCopyOfLargeValue, "implicit_copy_of_large_value", "W1006"),
        // W2001
        (ToolNotice, "tool_notice", "W2001"),
        // W2002
        (BindgenSkippedDeclaration, "bindgen_skipped_declaration", "W2002"),
    }
}
```

## B.2 コードごとの件数

| コード | kind の数 | 発行箇所の行 | 重要度 |
| --- | --- | --- | --- |
| `E0001` | 25 | 28 | error |
| `E0002` | 115 | 153 | error |
| `E0003` | 1 | 6 | error |
| `E1001` | 7 | 9 | error |
| `E1002` | 6 | 6 | error |
| `E1003` | 5 | 7 | error |
| `E1004` | 13 | 31 | error |
| `E1005` | 33 | 42 | error |
| `E1006` | 6 | 6 | error |
| `E1007` | 4 | 6 | error |
| `E1008` | 11 | 14 | error |
| `E1009` | 7 | 7 | error |
| `E1010` | 5 | 8 | error |
| `E1011` | 27 | 32 | error |
| `E1012` | 10 | 13 | error |
| `E1013` | 57 | 66 | error |
| `E1014` | 7 | 9 | error |
| `E1015` | 29 | 33 | error |
| `E1016` | 17 | 18 | error |
| `E1017` | 39 | 54 | error |
| `E1018` | 22 | 67 | error |
| `E1019` | 3 | 3 | error |
| `E1020` | 17 | 18 | error |
| `E1021` | 1 | 1 | error |
| `E1022` | 7 | 17 | error |
| `E1023` | 1 | 1 | error |
| `E1024` | 10 | 15 | error |
| `E1025` | 6 | 7 | error |
| `E1026` | 22 | 37 | error |
| `E1027` | 9 | 9 | error |
| `E1028` | 1 | 1 | error |
| `E2000` | 49 | 232 | error |
| `E2001` | 9 | 12 | error |
| `E2002` | 10 | 27 | error |
| `E2003` | 14 | 19 | error |
| `E2004` | 7 | 11 | error |
| `E2005` | 4 | 4 | error |
| `E2006` | 1 | 1 | error |
| `E2007` | 27 | 36 | error |
| `W1001` | 1 | 1 | warning |
| `W1002` | 2 | 2 | warning |
| `W1003` | 1 | 1 | warning |
| `W1004` | 1 | 1 | warning |
| `W1005` | 1 | 1 | warning |
| `W1006` | 1 | 1 | warning |
| `W2001` | 1 | 4 | warning |
| `W2002` | 1 | 1 | warning |

## B.3 kind の一覧（コード・kind・行数・ファイル・代表のメッセージ）

★ は手順 2.20 以降で専用ページを書く厳選 kind（付録 A.2）、☆ は Phase 1 の 8 kind です。`ファイル` は行を持つファイルと行数です。

| コード | kind | 行 | ファイル | 代表のメッセージ |
| --- | --- | --- | --- | --- |
| `E0001` | `bigint_suffix_with_fraction` | 1 | lexer | the 'I' suffix makes a bigint, which has no  |
| `E0001` | `byte_literal_not_ascii` | 2 | lexer×2 | a 'B' literal holds only ASCII characters (U |
| `E0001` | `byte_suffix_on_interpolated_string` | 1 | lexer | an interpolated string does not take the 'B' |
| `E0001` | `byte_suffix_on_unsupported_literal` | 1 | lexer | only 'c'B, "text"B, and u8"text"B take the ' |
| `E0001` | `char_literal_not_one_unit`★ | 1 | lexer | a char literal contains exactly one UTF-16 c |
| `E0001` | `comment_in_interpolation_hole` | 1 | lexer | comments are not allowed inside an interpola |
| `E0001` | `expected_digit` | 1 | lexer | expected a digit |
| `E0001` | `float_suffix_on_hex_or_binary_literal` | 1 | lexer | a hexadecimal or binary literal takes an int |
| `E0001` | `format_field_out_of_range` | 1 | lexer | format width and precision are at most {MAX_ |
| `E0001` | `format_precision_on_integer` | 1 | lexer | precision does not apply to integer format ' |
| `E0001` | `format_type_needs_precision` | 1 | lexer | format type '{type_char}' needs a precision  |
| `E0001` | `interpolation_hole_spans_lines` | 1 | lexer | an interpolation hole must stay on one line; |
| `E0001` | `invalid_format_spec` | 1 | lexer | invalid format spec '{text}'; expected [[fil |
| `E0001` | `invalid_numeric_literal`★ | 1 | lexer | invalid numeric literal or type suffix |
| `E0001` | `invalid_string_escape`★ | 1 | lexer | invalid string escape |
| `E0001` | `invalid_type_variable_start` | 1 | lexer | a type variable starts with an apostrophe an |
| `E0001` | `invalid_unicode_scalar_escape` | 1 | lexer | invalid Unicode scalar escape |
| `E0001` | `numeric_separator_misplaced` | 1 | lexer | numeric separators must occur between digits |
| `E0001` | `raw_newline_in_string` | 1 | lexer | use '\ ' for a newline inside a string |
| `E0001` | `single_closing_brace_in_interpolation` | 1 | lexer | a single '}' in an interpolated string must  |
| `E0001` | `unexpected_character`★ | 1 | lexer | unexpected character {character:?}; identifi |
| `E0001` | `unterminated_block_comment` | 1 | lexer | unterminated block comment |
| `E0001` | `unterminated_interpolation_hole` | 2 | lexer×2 | unterminated interpolation hole; close it wi |
| `E0001` | `unterminated_string_literal`★ | 2 | lexer×2 | unterminated string literal |
| `E0001` | `utf8char_not_one_scalar` | 1 | lexer | a utf8char literal contains exactly one Unic |
| `E0002` | `alias_name_on_same_line` | 1 | parser | write the alias name on the same line as '@a |
| `E0002` | `angle_bracket_spacing` | 2 | parser×2 | '<' must immediately follow the name |
| `E0002` | `builder_alias_is_contextual_keyword` | 1 | parser | '{}' is a contextual keyword and cannot be a |
| `E0002` | `builder_alias_name_case` | 1 | parser | a builder alias starts with a lowercase lett |
| `E0002` | `checked_let_rec` | 3 | parser×3 | <<CHECKED_LET_REC>> |
| `E0002` | `class_needs_one_type_parameter` | 1 | parser | a type class requires exactly one type param |
| `E0002` | `computation_match_without_arms` | 1 | parser | match! requires at least one '\| pattern ->  |
| `E0002` | `computation_nesting_too_deep` | 1 | computation | computation expansion exceeds depth {MAX_NES |
| `E0002` | `const_export_order` | 1 | parser | write 'export const def' to export a const f |
| `E0002` | `constraint_line_indentation` | 1 | parser | constraint lines must be indented after the  |
| `E0002` | `cpu_attribute_on_const_def` | 1 | parser | '@cpu' cannot apply to a 'const def'; the co |
| `E0002` | `definition_without_signature`★ | 1 | parser | function definition needs a 'def name :: ... |
| `E0002` | `do_bang_outside_task` | 1 | parser | 'do!' is only allowed inside a task block |
| `E0002` | `duplicate_builder_alias` | 1 | parser | builder alias '{}' is declared twice in this |
| `E0002` | `duplicate_cpu_target` | 1 | parser | CPU target '{name}' is listed twice |
| `E0002` | `duplicate_json_attribute` | 1 | parser | a {place} takes one '@json' attribute |
| `E0002` | `duplicate_manifest_key` | 2 | package×2 | wasm keys must occur at most once |
| `E0002` | `dyn_with_type_arguments` | 1 | parser | dyn takes a class name without type argument |
| `E0002` | `empty_interpolation_hole` | 1 | parse_control | an interpolation hole needs an expression; w |
| `E0002` | `empty_region_list` | 1 | parser | a region list must not be empty |
| `E0002` | `expected_binding_end`★ | 1 | parser | expected ';' or a newline after the binding |
| `E0002` | `expected_body_expression` | 1 | parse_control | expected a body expression |
| `E0002` | `expected_builder_alias` | 1 | parser | '@alias' takes a builder alias such as 'asyn |
| `E0002` | `expected_computation_body` | 1 | parser | expected a computation body |
| `E0002` | `expected_cpu_target` | 1 | parser | expected a CPU target such as "avx2" |
| `E0002` | `expected_dyn_class` | 1 | parser | expected a type class name after dyn |
| `E0002` | `expected_expression`★ | 1 | parser | expected an expression |
| `E0002` | `expected_expression_after_attribute` | 1 | parse_control | expected an expression; '@checked' is the on |
| `E0002` | `expected_identifier`★ | 1 | parser | expected an identifier |
| `E0002` | `expected_interpolation_close` | 1 | parse_control | expected '}' to close the interpolation hole |
| `E0002` | `expected_kind` | 1 | parser | expected '*' or a parenthesized kind |
| `E0002` | `expected_line_break`★ | 2 | parser×2 | expected a line break after the builder alia |
| `E0002` | `expected_link_name_string` | 1 | parser | expected a string literal {kind} name |
| `E0002` | `expected_pattern`★ | 1 | parse_control | expected a pattern |
| `E0002` | `expected_single_type_argument` | 1 | parser | expected exactly one type argument |
| `E0002` | `expected_token`★ | 1 | parser | expected {description} |
| `E0002` | `expected_type_close` | 1 | parser | expected '>' after type parameters or argume |
| `E0002` | `expected_union_case_name` | 1 | parser | expected a union case name |
| `E0002` | `expected_where_bindings` | 1 | parse_control | expected indented bindings after 'where' |
| `E0002` | `export_on_implementation` | 1 | parser | put 'export' on the function signature |
| `E0002` | `expression_nesting_too_deep` | 1 | parser | expression depth exceeds {MAX_NESTING}; intr |
| `E0002` | `extern_type_with_body` | 1 | parser | extern type declarations have no type parame |
| `E0002` | `extern_type_with_link_name` | 1 | parser | extern type declarations take no link name;  |
| `E0002` | `for_loop_needs_identifier` | 1 | parse_control | a for...to loop requires an identifier |
| `E0002` | `header_path_on_same_line` | 1 | parser | write the {keyword} path on the same line as |
| `E0002` | `header_path_spacing` | 1 | parser | join {keyword} path segments with '::' and n |
| `E0002` | `integer_literal_too_large` | 1 | parser | integer literal is too large |
| `E0002` | `invalid_dependency_form` | 1 | package | expected a path, git, or version dependency |
| `E0002` | `invalid_dependency_native_flag` | 1 | package | native must be true or false |
| `E0002` | `invalid_dependency_path` | 1 | package | dependency path must be a nonempty relative  |
| `E0002` | `invalid_fixed_array_length` | 1 | parser | a fixed array length must be an integer lite |
| `E0002` | `invalid_git_dependency` | 6 | package×6 | <<GIT_FORM>> |
| `E0002` | `invalid_json_attribute` | 1 | parser | expected the JSON name as a string literal a |
| `E0002` | `invalid_link_name_text` | 2 | parser×2 | extern link names must contain valid Unicode |
| `E0002` | `invalid_native_library_name` | 1 | package | <<&message>> |
| `E0002` | `invalid_native_path` | 1 | package | link and search paths must be nonempty relat |
| `E0002` | `invalid_native_section` | 3 | package×3 | <<FORM>> |
| `E0002` | `invalid_package_field` | 1 | package | package fields must be nonempty and occur ex |
| `E0002` | `invalid_registry_dependency` | 3 | package×3 | <<VERSION_FORM>> |
| `E0002` | `invalid_registry_setting` | 1 | package | <<rule>> |
| `E0002` | `invalid_wasm_size` | 1 | package | WASM sizes must be a quoted byte count or a  |
| `E0002` | `lambda_without_parameters` | 1 | parse_control | a lambda requires at least one parameter; us |
| `E0002` | `length_parameter_type` | 1 | parser | a length parameter has type 'i64', as in 'co |
| `E0002` | `let_function_needs_lambda`★ | 2 | parser×2 | a declared top-level 'let' function needs a  |
| `E0002` | `let_rec_needs_lambda` | 1 | parser | <<REC_NEEDS_LAMBDA>> |
| `E0002` | `list_type_with_length` | 1 | parser | list types use '[\|T\|]', without a length;  |
| `E0002` | `malformed_constraint_line` | 4 | parser×4 | expected a type variable after '@' on the sa |
| `E0002` | `malformed_multi_case_active_pattern` | 2 | parser×2 | multi-case active patterns use 'def (\|First |
| `E0002` | `malformed_new_collection` | 2 | parser×2 | expected an array or list after 'new': 'new  |
| `E0002` | `manifest_section_order` | 2 | package×2 | expected [package] followed by optional [dep |
| `E0002` | `manifest_syntax_error` | 6 | package×6 | expected '{token}' |
| `E0002` | `match_without_arms`★ | 1 | parse_control | expected at least one '\| pattern -> express |
| `E0002` | `misplaced_checked_attribute` | 3 | parse_control, parser×2 | '@checked' on its own line applies to the ne |
| `E0002` | `misplaced_const_modifier` | 1 | parser | put 'const' on the 'def' signature; its 'fn' |
| `E0002` | `misplaced_cpu_attribute` | 1 | parser | '@cpu' applies to the 'def' signature of a t |
| `E0002` | `misplaced_doc_comment`★ | 4 | parser×4 | doc comments attach to API declarations such |
| `E0002` | `misplaced_json_attribute` | 1 | parser | only '@json "name"' can come before a {place |
| `E0002` | `misplaced_namespace_declaration` | 1 | parser | a namespace declaration must be the first de |
| `E0002` | `misplaced_region_list` | 1 | parser | write one region list before a function type |
| `E0002` | `misplaced_using_declaration` | 1 | parser | using declarations belong at the top of the  |
| `E0002` | `missing_package_field` | 1 | package | [package] requires name and version |
| `E0002` | `missing_registry_index` | 1 | package | [registry] requires index = "https://..." |
| `E0002` | `module_declaration_unsupported` | 1 | parser | module declarations are not supported; each  |
| `E0002` | `mut_as_expression`★ | 1 | parser | 'mut' is not an expression; write 'ref mut x |
| `E0002` | `mutable_let_rec` | 1 | parser | <<REC_MUTABLE>> |
| `E0002` | `negative_pattern_needs_number` | 1 | parse_control | a negative pattern requires a numeric litera |
| `E0002` | `pattern_nesting_too_deep` | 1 | parse_control | pattern nesting exceeds the compiler limit |
| `E0002` | `prefix_keyword_operand` | 1 | parser | '{keyword}' takes exactly one operand; write |
| `E0002` | `recursive_let_bang` | 3 | parser×3 | <<LET_BANG_REC>> |
| `E0002` | `recursive_where_binding` | 1 | parse_control | 'where' bindings cannot be recursive; define |
| `E0002` | `return_not_last_in_computation` | 1 | parser | 'return' and 'return!' must end a computatio |
| `E0002` | `return_outside_task_end` | 1 | parser | 'return' is only allowed at the end of a tas |
| `E0002` | `signature_uses_def`★ | 1 | parser | use 'def name :: ...' for a signature; 'fn'  |
| `E0002` | `signature_uses_double_colon`★ | 1 | parser | use '::' between a 'def' name and its type,  |
| `E0002` | `signature_without_definition`★ | 1 | parser | function signature has no definition |
| `E0002` | `slice_must_be_borrowed`★ | 1 | parser | array slices must be borrows; write 'ref xs[ |
| `E0002` | `slice_without_bounds` | 1 | parser | a slice needs at least one bound; write 'ref |
| `E0002` | `stray_and_member` | 1 | parser | <<STRAY_MEMBER>> |
| `E0002` | `syntax_nesting_too_deep`★ | 1 | parser | syntax nesting exceeds {MAX_NESTING} |
| `E0002` | `too_many_active_pattern_cases` | 1 | parse_control | too many active pattern cases |
| `E0002` | `too_many_dyn_classes` | 1 | parser | too many classes after dyn |
| `E0002` | `too_many_extern_link_names` | 1 | parser | extern takes at most two link name strings;  |
| `E0002` | `too_many_function_parameters` | 1 | parser | too many function parameters |
| `E0002` | `too_many_interpolation_holes` | 1 | parse_control | an interpolated string has at most {MAX_INTE |
| `E0002` | `too_many_lambda_parameters` | 1 | parse_control | too many lambda parameters |
| `E0002` | `too_many_tuple_elements` | 1 | parse_control | too many tuple elements |
| `E0002` | `too_many_type_arguments` | 2 | parser×2 | too many type parameters or arguments |
| `E0002` | `type_parameters_need_angle_brackets` | 1 | parser | enclose type parameters in '<...>', as in Na |
| `E0002` | `typed_and_needs_body` | 1 | parser | a typed 'and' declaration requires '= ...' |
| `E0002` | `unknown_cpu_target` | 1 | parser | unknown CPU target '{name}'; use "sse4.2", " |
| `E0002` | `unknown_manifest_key` | 4 | package×4 | unknown package key; expected name, version, |
| `E0002` | `unsupported_edition` | 1 | package | unsupported edition '{value}'; supported edi |
| `E0002` | `use_bang_in_and_group` | 1 | parser | 'use!' cannot start an and! group; bind the  |
| `E0002` | `use_bang_outside_computation` | 1 | parser | 'use!' is only allowed inside a task block o |
| `E0002` | `where_binding_indentation` | 1 | parse_control | where bindings must have the same indentatio |
| `E0003` | `source_too_large`★ | 6 | driver×2, fetch, lexer, repl×2 | source exceeds the {MAX_SOURCE_BYTES}-byte l |
| `E1001` | `duplicate_and_binding` | 1 | parser | duplicate and! binding name |
| `E1001` | `duplicate_derived_class` | 1 | parser | duplicate derived class |
| `E1001` | `duplicate_export_name` | 1 | check | duplicate export 'tz_{}'; C/WASM export name |
| `E1001` | `duplicate_extern_declaration` | 1 | parser | duplicate extern or function declaration |
| `E1001` | `duplicate_function_definition`★ | 3 | parser×3 | duplicate function definition |
| `E1001` | `duplicate_function_signature` | 1 | parser | duplicate function signature |
| `E1001` | `duplicate_name`★ | 1 | check | duplicate or reserved name '{}' |
| `E1002` | `class_has_no_method`★ | 1 | polymorph | type class '{class_name}' has no method '{}' |
| `E1002` | `unknown_function`★ | 1 | check | unknown function '{}'{hint} |
| `E1002` | `unknown_length_constant` | 1 | check | unknown constant '{name}'; a length is an in |
| `E1002` | `unknown_module_member`☆ | 1 | check | <<message>> |
| `E1002` | `unknown_union_case`★ | 1 | check | union '{}' has no case '{name}' |
| `E1002` | `unknown_value`☆ | 1 | check | unknown value '{}' |
| `E1003` | `active_recognizer_result` | 2 | check×2 | a partial active recognizer must return bool |
| `E1003` | `fixed_array_literal_length` | 1 | check | expected {} elements for '{}', found {found} |
| `E1003` | `format_spec_type_mismatch` | 1 | check | format spec '{spelled}' does not apply to {} |
| `E1003` | `format_spec_type_unknown` | 1 | check | format spec '{spelled}' needs a concrete num |
| `E1003` | `type_mismatch`☆ | 2 | polymorph×2 | expected {}, found {} |
| `E1004` | `ambiguous_builder_alias` | 1 | check | builder alias '{written}' is ambiguous: the  |
| `E1004` | `ambiguous_name`★ | 6 | check×6 | '{name}' is ambiguous: using declarations ma |
| `E1004` | `ambiguous_pattern_name` | 1 | control | '{}' names both the union case '{}' and an a |
| `E1004` | `dyn_needs_type_class` | 1 | check | {kind}; dyn needs a type class such as dyn S |
| `E1004` | `empty_list_needs_annotation`★ | 1 | check | an empty list needs a type annotation, for e |
| `E1004` | `expected_length` | 1 | check | expected a length, such as '3', an 'i64' con |
| `E1004` | `length_used_as_type` | 1 | check | expected a type, found the length {length};  |
| `E1004` | `not_a_record_type` | 3 | check×3 | '{name}' names the union type '{}', not a re |
| `E1004` | `repeated_module_name` | 1 | check | '{path}' repeats the module name; the {kind} |
| `E1004` | `std_item_missing` | 6 | check, exceptions×2, polymorph×3 | try needs the standard library function Exce |
| `E1004` | `type_argument_count`★ | 5 | check×5 | Vec expects exactly one element type |
| `E1004` | `type_used_as_length` | 1 | check | expected a length, found the type '{name}' |
| `E1004` | `unknown_type_name`★ | 3 | check×3 | unknown type class, record type, or union ty |
| `E1005` | `atomic_element_type` | 1 | polymorph | atomic values must be i8, i16, i32, i64, i8u |
| `E1005` | `call_non_function`★ | 2 | polymorph×2 | only a function value can be called |
| `E1005` | `capture_exclusive_in_reusable_function` | 1 | polymorph | cannot capture {} in a reusable function; fu |
| `E1005` | `capture_rc_in_function_value` | 1 | polymorph | cannot capture {} in a function value; funct |
| `E1005` | `capture_sync_primitive_in_function_value` | 1 | polymorph | cannot capture {} in a function value; a fun |
| `E1005` | `capture_unsendable_in_function_value` | 1 | polymorph | cannot capture {ty} in a function value; fun |
| `E1005` | `collection_element_exclusive_borrow` | 1 | check | array and list elements cannot contain mutab |
| `E1005` | `cpu_function_wide_vector_boundary` | 3 | check×2, llvm_cpu | a '@cpu' function cannot pass a SIMD vector  |
| `E1005` | `deref_non_reference`★ | 1 | check | dereference requires a reference |
| `E1005` | `exclusive_slice_whole_array_parameter` | 1 | check | an exclusive slice cannot be passed as 'ref  |
| `E1005` | `fixed_array_init_annotation` | 1 | polymorph | 'FixedArray.init' creates a fixed-length arr |
| `E1005` | `fixed_slice_source` | 1 | check | slicing a fixed-length array requires a name |
| `E1005` | `for_over_fixed_array` | 1 | control | a fixed-length array is not enumerable; iter |
| `E1005` | `for_source_not_iterable` | 1 | control | for...in expects an array, list, string, utf |
| `E1005` | `function_constraint_unresolvable` | 1 | check | {} has no declaring module for function cons |
| `E1005` | `function_constraint_unsatisfied` | 1 | check | {} does not satisfy '#{name}': module '{}' h |
| `E1005` | `index_non_indexable`★ | 1 | check | indexing requires an array, list, string, or |
| `E1005` | `invalid_length_constant` | 1 | check | constant '{name}' cannot be a length; a leng |
| `E1005` | `new_on_fixed_array` | 1 | check | 'new' creates a heap array or list; remove ' |
| `E1005` | `no_instance`★ | 2 | polymorph×2 | no instance for {}<{}>; define an instance o |
| `E1005` | `no_instance_for_operator` | 1 | polymorph | no instance for this operator |
| `E1005` | `no_wider_integer` | 1 | polymorph | 128-bit integers have no wider integer type |
| `E1005` | `range_as_value` | 1 | check | a range is an enumerable expression for 'for |
| `E1005` | `rec_function_captures_generic` | 1 | polymorph | a 'let rec' function cannot capture '{}', wh |
| `E1005` | `record_update_non_record` | 1 | check | record update requires a record value |
| `E1005` | `shared_container_exclusive_borrow` | 1 | check | {} cannot hold a mutable reference; its owne |
| `E1005` | `simd_lane_count_mismatch` | 1 | polymorph | of_lanes count does not match the vector typ |
| `E1005` | `simd_operation_vector_type` | 1 | polymorph | this SIMD operation needs a concrete vector  |
| `E1005` | `slice_non_array` | 1 | check | a slice requires an array; lists and strings |
| `E1005` | `slice_of_fixed_array` | 1 | check | a fixed-length array has no exclusive slices |
| `E1005` | `std_representation_mismatch` | 6 | polymorph×6 | Seq.next requires the standard Seq record |
| `E1005` | `unsized_slice_type_position` | 1 | check | '[T..]' is only valid directly after 'ref mu |
| `E1005` | `use_requires_drop` | 1 | check | 'use' needs a value whose type implements Dr |
| `E1006` | `active_pattern_argument_count` | 1 | control | recognizer '{}' expects {extras} arguments{} |
| `E1006` | `active_recognizer_parameters` | 1 | check | an active recognizer needs an input paramete |
| `E1006` | `definition_parameter_count_mismatch`★ | 1 | parser | signature expects {} parameters, definition  |
| `E1006` | `instance_method_parameter_count` | 1 | polymorph | method '{}' expects {} parameters |
| `E1006` | `owned_function_parameter_count` | 1 | closures | Owned.function takes a lambda with one param |
| `E1006` | `too_many_arguments_applied`★ | 1 | polymorph | cannot apply {count} arguments to a function |
| `E1007` | `field_access_non_record`★ | 1 | check | field access requires a record, or '.length' |
| `E1007` | `missing_record_fields`★ | 1 | check | missing fields: {} |
| `E1007` | `record_has_no_field`★ | 3 | check×2, control | record has no field '{}' |
| `E1007` | `unknown_field`★ | 1 | check | unknown field '{}' |
| `E1008` | `conflicting_extern_declaration` | 1 | check | extern symbol '{}' is declared with differen |
| `E1008` | `export_active_recognizer` | 1 | check | active recognizers cannot be exported; expos |
| `E1008` | `export_on_extern_declaration` | 2 | parser×2 | extern declarations cannot be exported |
| `E1008` | `extern_callback_argument` | 1 | check | callback arguments must name a top-level use |
| `E1008` | `extern_callback_call_form` | 1 | check | extern '{}' takes a callback and must be cal |
| `E1008` | `extern_declaration_constraints` | 1 | check | extern declarations cannot have constraints  |
| `E1008` | `invalid_extern_symbol` | 1 | abi | invalid extern symbol '{symbol}'; use a C id |
| `E1008` | `invalid_wasm_import_module` | 1 | abi | invalid WASM import module '{module}'; use 1 |
| `E1008` | `reserved_extern_symbol` | 2 | abi×2 | extern symbol '{symbol}' is reserved by Tsuz |
| `E1008` | `unsupported_export_signature` | 1 | check | exports support scalar values, borrowed i64/ |
| `E1008` | `unsupported_extern_signature` | 2 | check×2 | extern callback parameters support functions |
| `E1009` | `decimal_literal_out_of_range` | 1 | numeric | decimal literal is out of range |
| `E1009` | `float_literal_out_of_range` | 1 | numeric | floating-point literal is out of range |
| `E1009` | `float_literal_type` | 1 | check | a floating-point literal needs a binary or d |
| `E1009` | `integer_literal_out_of_range`★ | 1 | check | integer literal is outside the range of {} |
| `E1009` | `integer_literal_type` | 1 | check | an integer literal needs an integer type |
| `E1009` | `invalid_bigint_digit` | 1 | exceptions | invalid digit in a bigint literal |
| `E1009` | `invalid_float_literal` | 1 | numeric | invalid floating-point literal |
| `E1010` | `fixed_array_too_long` | 1 | check | fixed array length {length} exceeds {MAX_FIX |
| `E1010` | `infinite_recursive_layout` | 3 | check×2, recursive | recursive value layout must pass through a u |
| `E1010` | `layout_nesting_limit` | 2 | check×2 | record nesting exceeds {MAX_NESTING} |
| `E1010` | `recursive_union_without_base_case` | 1 | recursive | recursive union has no finite value; add a b |
| `E1010` | `value_layout_too_large` | 1 | check | value layout exceeds {MAX_VALUE_BYTES} bytes |
| `E1011` | `ambiguous_package_root` | 1 | driver | package name or namespace resolves to multip |
| `E1011` | `conflicting_dependency_source` | 1 | driver | package '{name}' is required from different  |
| `E1011` | `cyclic_package_dependency` | 1 | driver | cyclic package dependency |
| `E1011` | `dependency_kind_not_allowed` | 2 | driver×2 | registry packages can depend only on registr |
| `E1011` | `dependency_name_mismatch` | 1 | driver | dependency key must match the dependency pac |
| `E1011` | `dependency_path_not_relative` | 1 | driver | dependency paths must be relative |
| `E1011` | `duplicate_dependency` | 1 | package | duplicate package dependency |
| `E1011` | `duplicate_module`★ | 1 | check | module '{}' is declared by more than one fil |
| `E1011` | `incompatible_dependency_versions` | 1 | fetch | package '{}' is required at incompatible ver |
| `E1011` | `invalid_module_filename` | 1 | driver | the module filename must be an ASCII identif |
| `E1011` | `invalid_module_name`★ | 2 | check×2 | module name '{stem}' must start with an uppe |
| `E1011` | `invalid_package_name` | 1 | package | package names must be lowercase ASCII kebab- |
| `E1011` | `invalid_package_namespace` | 1 | package | package namespace must be identifiers joined |
| `E1011` | `invalid_publish_version` | 1 | fetch | a published package needs [package] version  |
| `E1011` | `invalid_source_path` | 1 | lib | source paths must be relative ASCII module s |
| `E1011` | `invalid_std_path` | 1 | lib | invalid standard library path '{}'; std sour |
| `E1011` | `invalid_using_declaration` | 1 | check | <<message>> |
| `E1011` | `manifest_not_regular_file` | 1 | driver | package manifests must be regular files, not |
| `E1011` | `module_namespace_collision` | 1 | driver | root module collides with a dependency names |
| `E1011` | `module_source_not_regular_file` | 1 | driver | module sources must be regular files |
| `E1011` | `publish_non_registry_dependency` | 1 | fetch | a published package can depend only on regis |
| `E1011` | `reserved_package_namespace` | 1 | driver | package namespace is reserved by the standar |
| `E1011` | `reserved_std_namespace`★ | 2 | check×2 | <<message>> |
| `E1011` | `source_filename_case_mismatch` | 1 | driver | source filename must match its directory ent |
| `E1011` | `source_outside_package_root` | 1 | driver | source is outside its package root |
| `E1011` | `source_without_filename` | 1 | driver | a source file needs a filename |
| `E1011` | `symlink_not_followed` | 3 | driver×3 | module discovery does not follow source or d |
| `E1012` | `assign_to_immutable_place`★ | 1 | check | assignment replaces a mutable binding; recor |
| `E1012` | `drop_pass_exclusive_value` | 1 | ownership | cannot pass the value on as 'ref mut' inside |
| `E1012` | `drop_replace_whole_value` | 1 | ownership | cannot replace the whole value inside Drop.d |
| `E1012` | `move_out_of_collection_element`★ | 3 | ownership×3 | cannot move a non-Copy element out of an arr |
| `E1012` | `move_out_of_drop_type` | 2 | ownership×2 | <<MOVE_OUT_OF_DROP>> |
| `E1012` | `move_out_of_owned_function` | 1 | closures | cannot move '{}' out of an owned function; b |
| `E1012` | `move_out_of_rec_function` | 1 | closures_rec | cannot move '{}' out of a 'let rec' function |
| `E1012` | `move_out_of_reference_binding` | 1 | ownership | cannot move a non-Copy pattern variable boun |
| `E1012` | `update_drop_type_value`★ | 1 | ownership | cannot update a value whose type implements  |
| `E1012` | `use_after_move`☆ | 1 | ownership | use of moved or partially moved value '{name |
| `E1013` | `assignment_extends_borrow` | 1 | ownership | assignment lets a reference outlive its owne |
| `E1013` | `borrow_across_async_boundary` | 1 | ownership | <<ASYNC_BORROW>> |
| `E1013` | `borrow_field_without_region` | 1 | regions | field '{}' stores a borrow without a region; |
| `E1013` | `borrow_of_temporary`★ | 1 | ownership | borrow requires a local place; bind the temp |
| `E1013` | `borrow_outlives_binding` | 1 | ownership_control | a borrowed value cannot outlive its pattern  |
| `E1013` | `borrow_outlives_block`☆ | 1 | ownership | borrowed value does not live long enough to  |
| `E1013` | `borrowed_union_payload` | 1 | check | union payloads are owned values; borrowed pa |
| `E1013` | `callback_contract_unsatisfied` | 1 | ownership | <<message>> |
| `E1013` | `channel_item_holds_borrow` | 1 | ownership | a channel item cannot hold a borrow, includi |
| `E1013` | `const_temporary_borrow_escapes` | 1 | constants | a temporary const borrow cannot escape or cr |
| `E1013` | `dyn_holds_borrow_without_region` | 2 | ownership, polymorph | a dyn value without a region cannot hold bor |
| `E1013` | `dyn_holds_exclusive_borrow` | 1 | polymorph | a dyn value cannot hold exclusive borrows, a |
| `E1013` | `exclusive_borrow_behind_region` | 1 | regions | the data behind a reference with its own tar |
| `E1013` | `exclusive_borrow_field_shape` | 2 | check×2 | <<message>> |
| `E1013` | `exclusive_field_regions_missing` | 1 | regions | field '{name}' stores an exclusive borrow in |
| `E1013` | `exclusive_field_without_region` | 1 | regions | exclusive borrow field '{name}' needs a name |
| `E1013` | `exclusive_region_reused` | 1 | regions | region '{}' belongs to an exclusive borrow,  |
| `E1013` | `field_mixes_regions` | 1 | regions | one field cannot mix distinct named regions; |
| `E1013` | `invalid_region_names` | 1 | parser | region names must be distinct lowercase iden |
| `E1013` | `misplaced_region_quantifier` | 1 | regions | region-quantified function types are only su |
| `E1013` | `multiple_regions_on_value` | 1 | regions | each borrowed value has one named region; sp |
| `E1013` | `must_apply_directly` | 3 | ownership×3 | parallel operations must be fully applied di |
| `E1013` | `mutable_record_field` | 1 | check | record fields are immutable; use an owned up |
| `E1013` | `mutable_region_quantified_parameter` | 1 | regions | a parameter with a region-quantified functio |
| `E1013` | `mutex_result_retains_borrow` | 1 | ownership | Mutex.with_lock results must be proven free  |
| `E1013` | `named_region_in_function_type` | 1 | regions | named regions inside higher-order function t |
| `E1013` | `named_region_in_local` | 1 | regions | named regions belong in function signatures  |
| `E1013` | `nested_borrow_unsupported` | 1 | ownership | nested borrowed values require explicit life |
| `E1013` | `owned_function_captures_reference` | 1 | closures | an owned function captures only owned values |
| `E1013` | `parallel_callback_borrows_environment` | 1 | ownership | parallel callbacks and values cannot retain  |
| `E1013` | `parallel_input_borrows_environment` | 1 | ownership | parallel input elements must have proven own |
| `E1013` | `parallel_result_retains_borrow` | 1 | ownership | parallel callback results must be proven fre |
| `E1013` | `reference_region_count` | 1 | parser | a reference has exactly one region |
| `E1013` | `reference_without_owner`★ | 1 | ownership | reference has no live owner |
| `E1013` | `region_already_declared` | 1 | regions | region '{}' is already declared by the funct |
| `E1013` | `region_belongs_to_function` | 1 | regions | region '{}' belongs to the function, not to  |
| `E1013` | `region_callback_stores_exclusive_input` | 1 | regions | '{name}' may store an input through an exclu |
| `E1013` | `region_count_mismatch` | 1 | regions | the record declares {count} regions; write e |
| `E1013` | `region_on_non_borrow` | 1 | regions | a named region requires a reference or an ag |
| `E1013` | `region_quantifier_target` | 1 | regions | a region quantifier applies to a function ty |
| `E1013` | `result_region_mismatch` | 1 | ownership | returned borrow does not match the declared  |
| `E1013` | `result_region_missing` | 2 | regions×2 | annotate the whole borrowed result with one  |
| `E1013` | `result_region_without_input` | 1 | regions | result region '{region}' has no matching inp |
| `E1013` | `self_referential_store` | 2 | ownership×2 | cannot store a reference inside its own owne |
| `E1013` | `store_local_borrow_through_reference` | 1 | ownership | cannot store a borrow of a local value throu |
| `E1013` | `store_region_mismatch` | 1 | ownership | the stored borrow does not have the region o |
| `E1013` | `store_region_unnamed` | 2 | ownership×2 | {UNNAMED}; name the target's region apart fr |
| `E1013` | `task_capture_retains_borrow`★ | 2 | ownership×2 | task captures cannot retain borrowed values, |
| `E1013` | `task_requires_owned_value` | 1 | polymorph | tasks require owned values; {} contains a re |
| `E1013` | `task_requires_send` | 2 | polymorph×2 | tasks require Send values; {} holds an Rc or |
| `E1013` | `task_requires_sync` | 1 | polymorph | tasks can share only Sync values; {} is not  |
| `E1013` | `task_result_is_reference` | 1 | check | task results must be owned values, not refer |
| `E1013` | `task_result_retains_borrow` | 1 | ownership | task results cannot retain borrowed values |
| `E1013` | `task_scope_shares_borrowed_environment` | 1 | ownership | task scopes can share only values with prove |
| `E1013` | `undeclared_region` | 1 | regions | undeclared region '{}'; add it after the dec |
| `E1013` | `union_payload_exclusive_borrow` | 1 | check | union payloads cannot store mutable referenc |
| `E1013` | `unused_region` | 1 | regions | unused region '{}'; remove it or use it in a |
| `E1014` | `active_pattern_mutates_input` | 1 | control | active recognizers cannot mutate the matched |
| `E1014` | `borrow_conflict`☆ | 1 | ownership | access conflicts with a live borrow; use the |
| `E1014` | `exclusive_slice_source` | 1 | check | an exclusive slice must borrow a named array |
| `E1014` | `move_reborrowed_exclusive_reference` | 1 | ownership | cannot move an exclusive reference while it  |
| `E1014` | `mutate_immutable_value`☆ | 1 | ownership | mutable access requires 'let mut' or an excl |
| `E1014` | `mutate_through_shared_reference`★ | 3 | check×2, ownership | cannot mutate or exclusively reborrow throug |
| `E1014` | `replace_exclusive_slice` | 1 | check | cannot replace an exclusive slice as a whole |
| `E1015` | `ambiguous_polymorphic_use`★ | 1 | polymorph | ambiguous polymorphic use; add a concrete ty |
| `E1015` | `concrete_integer_required` | 1 | polymorph | this builtin needs a concrete integer argume |
| `E1015` | `concrete_type_required`★ | 1 | polymorph | a concrete type is required here; add a type |
| `E1015` | `constructor_kind_mismatch` | 2 | higher_kinds, polymorph | type constructor kind does not match the cla |
| `E1015` | `constructor_variable_kind_annotation` | 1 | check | type constructor variable requires a matchin |
| `E1015` | `dyn_dispatch_result_type` | 1 | check | a dyn dispatch needs the method's result typ |
| `E1015` | `dyn_of_arity` | 1 | polymorph | Dyn.of must be applied to exactly one argume |
| `E1015` | `dyn_of_needs_expected_type` | 1 | check | Dyn.of needs an expected dyn type here; anno |
| `E1015` | `expected_type_constructor` | 1 | higher_kinds | expected a named type constructor with the d |
| `E1015` | `extern_signature_not_concrete` | 1 | check | extern signatures must be concrete; provide  |
| `E1015` | `extern_type_as_constructor` | 1 | higher_kinds | an extern type takes no type arguments, so i |
| `E1015` | `fixed_array_length_unknown` | 1 | polymorph | cannot determine the length of 'FixedArray.i |
| `E1015` | `higher_kinded_alias` | 1 | higher_kinds | higher-kinded type aliases are not supported |
| `E1015` | `higher_order_kind_argument` | 1 | higher_kinds | phase-1 constructors accept value types, not |
| `E1015` | `infinite_inferred_type`★ | 1 | polymorph | infinite inferred type |
| `E1015` | `instance_head_not_constructor` | 1 | polymorph | higher-kinded instances require a named cons |
| `E1015` | `internal_rec_binding_not_lambda` | 1 | check | internal compiler error: a 'let rec' binding |
| `E1015` | `internal_rec_group_unbound` | 1 | closures_rec | internal compiler error: the members of a 'l |
| `E1015` | `internal_rec_not_bound_by_block` | 1 | closures_rec | internal compiler error: a 'let rec' functio |
| `E1015` | `internal_unlowerable_ir` | 1 | llvm | internal compiler error: erroneous or unsatu |
| `E1015` | `missing_type_arguments` | 1 | check | recode: error.code = "E1015" for a non-alias |
| `E1015` | `polymorphic_entry_point` | 1 | polymorph | the application entry point cannot be polymo |
| `E1015` | `ref_operand_inferred_late` | 1 | polymorph | 'ref' must know whether its operand is a ref |
| `E1015` | `scalar_as_type_constructor` | 1 | higher_kinds | a scalar value type is not a type constructo |
| `E1015` | `simd_vector_type_unknown` | 1 | polymorph | this SIMD operation needs a concrete vector  |
| `E1015` | `superclass_kind_mismatch` | 1 | polymorph | superclass parameter kind does not match the |
| `E1015` | `type_constructor_not_applied` | 1 | check | a type constructor must be fully applied in  |
| `E1015` | `type_variable_absent_from_signature` | 4 | check×2, polymorph×2 | constraint mentions a type variable absent f |
| `E1015` | `type_variable_kind_conflict` | 1 | polymorph | type variable is constrained at incompatible |
| `E1016` | `builtin_instance_override` | 1 | polymorph | built-in instances and marker classes cannot |
| `E1016` | `class_constraint_arity` | 1 | check | type class '{}' constrains exactly one type, |
| `E1016` | `class_method_constraint` | 2 | polymorph×2 | class methods use the class parameter; metho |
| `E1016` | `class_method_type_variable` | 1 | polymorph | each class method must mention exactly the d |
| `E1016` | `class_without_methods` | 1 | polymorph | a class needs at least one method |
| `E1016` | `default_body_unknown_method` | 1 | polymorph | a default body must name a declared class me |
| `E1016` | `drop_instance_constraint` | 1 | polymorph | Drop instances cannot have constraints; drop |
| `E1016` | `drop_instance_generic` | 1 | polymorph | a Drop instance must cover every instantiati |
| `E1016` | `drop_instance_target` | 1 | polymorph | only records and unions declared in this pro |
| `E1016` | `explicit_drop_call` | 1 | polymorph | 'Drop.drop' runs automatically when a value  |
| `E1016` | `field_type_constraint` | 1 | check | {parts} cannot constrain type parameters; pu |
| `E1016` | `format_instance_target` | 1 | polymorph | only records and unions declared in this pro |
| `E1016` | `instance_missing_method`★ | 1 | polymorph | missing method '{}.{}' |
| `E1016` | `instance_unknown_method` | 1 | polymorph | unknown method '{}' for {} |
| `E1016` | `missing_explicit_constraint` | 1 | polymorph | '{}.{} requires an explicit @'{} : #{} const |
| `E1016` | `overlapping_instance`★ | 1 | polymorph | overlapping instance for {}<{}> |
| `E1016` | `unknown_type_class` | 1 | polymorph | unknown type class '{}' |
| `E1017` | `and_group_too_large` | 1 | parser | and! group exceeds the compiler binding limi |
| `E1017` | `class_constraint_limit` | 1 | polymorph | too many class constraints |
| `E1017` | `deriving_expression_limit` | 1 | derive | deriving exceeds the expression depth or nod |
| `E1017` | `deriving_instance_limit` | 1 | derive | deriving exceeds 1024 instances |
| `E1017` | `exhaustiveness_search_limit` | 1 | exhaustiveness | the match is too large to check for exhausti |
| `E1017` | `function_constraint_limit` | 2 | polymorph×2 | too many distinct function constraints in on |
| `E1017` | `git_dependency_too_large` | 1 | fetch | git dependency '{name}' has a tree listing l |
| `E1017` | `gpu_kernel_limit` | 3 | gpu×3 | GPU kernel exceeds 128 call levels or 1024 f |
| `E1017` | `header_path_too_long` | 1 | parser | {keyword} paths are limited to 16 segments |
| `E1017` | `instance_overlap_limit` | 1 | polymorph | instance overlap checking exceeds 1024 pairs |
| `E1017` | `instance_resolution_cycle` | 1 | polymorph | instance constraint resolution forms a cycle |
| `E1017` | `instance_resolution_limit` | 1 | polymorph | instance constraint resolution exceeds depth |
| `E1017` | `lockfile_too_large` | 2 | driver, package | Tsuzuri.lock exceeds 1 MiB |
| `E1017` | `loop_ownership_limit` | 1 | ownership_control | loop ownership analysis exceeds the compiler |
| `E1017` | `manifest_too_large` | 1 | package | package manifest exceeds 1 MiB |
| `E1017` | `module_discovery_limit` | 4 | driver×2, fetch×2 | module discovery exceeds 1024 directories or |
| `E1017` | `module_path_limit` | 1 | check | module paths are limited to 16 segments and  |
| `E1017` | `namespace_path_too_long` | 1 | parser | namespace paths are limited to 16 segments b |
| `E1017` | `or_pattern_alternatives_limit` | 1 | control | too many OR pattern alternatives |
| `E1017` | `package_graph_limit` | 2 | driver×2 | package graph exceeds 1024 packages or depth |
| `E1017` | `pattern_alternatives_limit` | 2 | control, exhaustiveness | pattern alternatives exceed the compiler lim |
| `E1017` | `polymorphic_constraint_limit` | 2 | polymorph×2 | too many distinct polymorphic constraints in |
| `E1017` | `polymorphic_recursion_growth` | 1 | polymorph | polymorphic recursion grows the types of '{n |
| `E1017` | `polymorphic_type_expansion_limit` | 1 | polymorph | polymorphic type expansion exceeds the compi |
| `E1017` | `qualified_name_too_long` | 2 | parser×2 | qualified names exceed the 18-segment compil |
| `E1017` | `recursive_type_changes_arguments` | 1 | recursive | recursive generic type changes its arguments |
| `E1017` | `recursive_type_expansion_limit` | 1 | recursive | recursive type expansion exceeds the compile |
| `E1017` | `registry_index_too_large` | 2 | fetch×2 | the registry index has a listing larger than |
| `E1017` | `registry_resolution_limit` | 1 | fetch | registry resolution exceeds {MAX_REGISTRY_PA |
| `E1017` | `sequence_loop_depth_limit` | 1 | control | sequence loop expansion exceeds the compiler |
| `E1017` | `specialization_limit` | 1 | polymorph | more than {} specializations of generic func |
| `E1017` | `superclass_resolution_limit` | 2 | polymorph×2 | superclass resolution exceeds depth 64 |
| `E1017` | `too_many_dependencies` | 1 | package | a package may have at most 1024 dependencies |
| `E1017` | `too_many_loop_exits` | 1 | ownership_control | too many loop exit paths; split the loop bod |
| `E1017` | `too_many_raise_points` | 1 | ownership_control | too many raise points in one try; split the  |
| `E1017` | `too_many_record_regions` | 1 | regions | too many regions in record '{}'; declare at  |
| `E1017` | `too_many_region_parameters` | 1 | parser | too many region parameters |
| `E1017` | `too_many_trap_locations` | 2 | llvm_traps×2 | too many trap locations |
| `E1017` | `type_alias_expansion_limit` | 2 | check×2 | type alias expansion exceeds the compiler li |
| `E1018` | `alias_outside_builder_file` | 1 | computation | '@alias {}' names a computation builder, so  |
| `E1018` | `bench_clock_outside_bench` | 2 | llvm×2 | {RULE}; {ADVICE} |
| `E1018` | `builder_missing_operation` | 1 | computation | computation builder '{}' does not define '{o |
| `E1018` | `builder_without_operations` | 1 | computation | a .tc builder needs at least one computation |
| `E1018` | `cannot_infer_builder` | 1 | computation | cannot infer the computation builder; add a  |
| `E1018` | `class_outside_class_file` | 1 | computation | type class declarations belong in a .tt file |
| `E1018` | `computation_without_builder` | 1 | computation | {what} needs a computation builder, but this |
| `E1018` | `declaration_in_class_file` | 1 | computation | a .tt file contains only type class declarat |
| `E1018` | `gpu_buffer_element_type` | 2 | gpu×2 | GPU operations require a concrete buffer res |
| `E1018` | `gpu_callback_not_known` | 4 | gpu×3, gpu/spirv | GPU init/map operations require a known func |
| `E1018` | `gpu_kernel_missing` | 1 | gpu | GPU kernel function does not exist |
| `E1018` | `gpu_kernel_recursion` | 1 | gpu | GPU kernels cannot call recursive functions |
| `E1018` | `gpu_kernel_signature` | 4 | gpu×2, gpu/spirv×2 | GPU kernels require one scalar parameter and |
| `E1018` | `gpu_operation_not_direct` | 3 | gpu×3 | GPU operations cannot escape as function val |
| `E1018` | `gpu_unsupported_construct` | 13 | gpu×3, gpu/spirv×10 | GPU kernels do not support allocation, borro |
| `E1018` | `gpu_unsupported_operator` | 14 | gpu×4, gpu/spirv×10 | WGSL kernels do not support '**'; multiply e |
| `E1018` | `gpu_unsupported_type` | 9 | gpu×5, gpu/spirv×4 | GPU kernels cannot allocate or carry nonscal |
| `E1018` | `implicit_task_needs_builder` | 1 | computation | this implicit task operation requires an exp |
| `E1018` | `no_builder_for_value` | 1 | computation | no computation builder has a Bind operation  |
| `E1018` | `std_declares_test` | 2 | check×2 | embedded standard-library sources cannot dec |
| `E1018` | `std_exports_function` | 1 | check | the standard library cannot export functions |
| `E1018` | `unknown_computation_builder` | 2 | computation×2 | unknown computation builder '{0}'; declare ' |
| `E1019` | `and_without_recursive_group`★ | 1 | parser | 'and' requires a preceding recursive declara |
| `E1019` | `mismatched_recursive_group` | 1 | parser | 'def' and its implementation must have match |
| `E1019` | `missing_rec_marker`★ | 1 | recursion | recursive function '{}' requires 'rec' on it |
| `E1020` | `active_argument_not_value` | 1 | control | a recognizer argument must be a value expres |
| `E1020` | `active_pattern_case_count` | 1 | check | active pattern and backing union must have t |
| `E1020` | `active_pattern_case_name` | 1 | parse_control | an active pattern case starts with an upperc |
| `E1020` | `array_pattern_on_fixed_array` | 1 | control | fixed-length arrays cannot be destructured b |
| `E1020` | `case_pattern_missing_payload`★ | 1 | control | union case '{}' carries a payload; match it  |
| `E1020` | `case_pattern_payload_shape` | 1 | control | union case '{}' carries one payload; match s |
| `E1020` | `case_pattern_unexpected_payload` | 1 | control | union case '{}' has no payload; remove the p |
| `E1020` | `duplicate_active_pattern_case` | 1 | parser | duplicate active pattern case '{}' |
| `E1020` | `expression_in_pattern` | 1 | control | expressions are only allowed as recognizer a |
| `E1020` | `multi_case_active_pattern_result_type` | 1 | parse_control | multi-case active patterns use the implicit  |
| `E1020` | `null_pattern` | 1 | parse_control | Tsuzuri has no null values; null patterns ar |
| `E1020` | `or_pattern_binding_mismatch`★ | 2 | control×2 | both sides of an OR pattern must bind the sa |
| `E1020` | `partial_active_pattern_case_count` | 1 | parse_control | partial active patterns have exactly one cas |
| `E1020` | `qualified_pattern_binding` | 1 | control | a pattern binding cannot be a qualified name |
| `E1020` | `record_pattern_unknown_type` | 1 | control | an unqualified record pattern needs a known  |
| `E1020` | `reserved_active_result_variable` | 1 | check | 'T is reserved for the implicit active-patte |
| `E1020` | `unknown_case_or_active_pattern` | 1 | control | unknown union case or active pattern '{}' |
| `E1021` | `non_exhaustive_match`☆ | 1 | exhaustiveness | match is not exhaustive; missing: {} |
| `E1022` | `opaque_type_representation` | 1 | check | the representation of '{}' is opaque; use it |
| `E1022` | `private_builder_operation` | 1 | computation | builder operation '{}' cannot be private; ma |
| `E1022` | `private_export` | 1 | parser | 'private export' is not allowed; remove 'pri |
| `E1022` | `private_name_access`★ | 1 | check | private {kind} '{name}' is only visible insi |
| `E1022` | `private_on_unsupported_declaration` | 1 | parser | <<message>> |
| `E1022` | `private_std_primitive` | 10 | polymorph×10 | IO primitives are private to the standard IO |
| `E1022` | `private_type_in_public_api`★ | 2 | check×2 | private type '{}' leaks from public {kind} ' |
| `E1023` | `jump_outside_loop` | 1 | check | {keyword} can only target an enclosing for o |
| `E1024` | `alias_argument_count` | 1 | check | type '{name}' expects {expected} type argume |
| `E1024` | `cyclic_type_alias` | 1 | check | cyclic type alias '{}'; remove the recursive |
| `E1024` | `duplicate_type_parameter` | 1 | check | duplicate {} in {kind} '{name}'; give each p |
| `E1024` | `duplicate_union_case` | 1 | check | duplicate case '{}' in union '{}'; give each |
| `E1024` | `invalid_type_parameter_name` | 1 | check | '_ cannot name a {kind} type parameter; use  |
| `E1024` | `type_name_case` | 2 | check×2 | a type alias name must start with an upperca |
| `E1024` | `undeclared_length_parameter` | 1 | check | length '{length}' is not declared by {kind}  |
| `E1024` | `undeclared_type_variable` | 3 | check×3 | type variable '{variable} is not declared by |
| `E1024` | `union_case_name_case` | 1 | check | union case '{}' must start with an uppercase |
| `E1024` | `unused_type_parameter` | 3 | check×3 | {} is not used by the alias target; remove i |
| `E1025` | `default_derive_recursive` | 1 | polymorph | derived Default follows a recursive first ca |
| `E1025` | `derived_instance_invalid` | 2 | polymorph×2 | recode when instance.derived (code E1025) |
| `E1025` | `deriving_unavailable` | 1 | derive | this deriving implementation is not availabl |
| `E1025` | `duplicate_json_name` | 1 | derive | the JSON name "{}" of {place} '{}' repeats a |
| `E1025` | `json_name_without_deriving` | 1 | derive | '@json' names a {place} only for deriving (E |
| `E1025` | `underivable_class` | 1 | parser | only Eq, Ord, Display, Hash, Default, Encode |
| `E1026` | `const_aggregate_operator` | 2 | const_rules×2 | an operator on a record, tuple, array, or un |
| `E1026` | `const_assignment_target` | 1 | const_rules | a const def assigns only to a 'let mut' vari |
| `E1026` | `const_borrow_not_allowed` | 1 | const_rules | a borrow is not allowed in a const def; read |
| `E1026` | `const_call_depth_limit` | 1 | const_machine | const evaluation exceeds the compiler limit  |
| `E1026` | `const_callee_not_const` | 2 | const_rules×2 | '{name}' is not a const def; mark it 'const  |
| `E1026` | `const_checked_operation` | 1 | const_rules | an '@checked' operation raises an exception  |
| `E1026` | `const_condition_not_bool` | 1 | const_machine | a const condition must be bool |
| `E1026` | `const_cycle` | 2 | const_machine, constants | cyclic const definition; break the cycle or  |
| `E1026` | `const_evaluation_trapped` | 3 | const_machine, constants×2 | const division trapped on zero or signed ove |
| `E1026` | `const_expression_not_allowed` | 1 | constants | this expression is not allowed in a phase-1  |
| `E1026` | `const_for_loop_form` | 1 | const_rules | a for loop in a const def enumerates an arra |
| `E1026` | `const_function_value` | 1 | const_rules | a function value is not allowed in a const d |
| `E1026` | `const_limit_exceeded` | 1 | constants | const evaluation exceeds the compiler limit; |
| `E1026` | `const_new_collection_form` | 1 | const_rules | new [T](n, f) in a const def takes a lambda  |
| `E1026` | `const_operation_not_allowed` | 2 | const_machine, const_rules | this operation is not allowed in a const def |
| `E1026` | `const_partial_application` | 1 | const_rules | a const def must be fully applied in const e |
| `E1026` | `const_pipe_not_allowed` | 1 | const_rules | the pipe operator is not allowed in a const  |
| `E1026` | `const_signature_invalid` | 2 | const_rules×2 | a const def must have a concrete signature;  |
| `E1026` | `const_static_data_too_large` | 1 | const_static | the constants that the program reads in plac |
| `E1026` | `const_unbuildable_type` | 1 | const_rules | a value of type '{}' cannot be built in a co |
| `E1026` | `const_unsupported_operation` | 9 | const_rules×2, constants×7 | this const unary operation is unsupported; u |
| `E1026` | `constant_type_not_concrete` | 1 | check | constant types must be concrete; remove type |
| `E1027` | `class_context_not_a_class` | 1 | polymorph | class contexts require type class constraint |
| `E1027` | `instance_check_failed` | 1 | polymorph | recode when not derived (code E1027) |
| `E1027` | `instance_context_variable` | 1 | polymorph | instance context mentions a type variable ab |
| `E1027` | `instance_missing_superclass` | 1 | polymorph | instance context does not entail its supercl |
| `E1027` | `superclass_cycle` | 1 | polymorph | superclass constraints form a cycle |
| `E1027` | `superclass_type_variable` | 1 | polymorph | superclasses may mention only the class type |
| `E1027` | `undeclared_default_constraint` | 1 | polymorph | default method requires an undeclared constr |
| `E1027` | `undeclared_instance_constraint` | 1 | polymorph | instance method requires an undeclared const |
| `E1027` | `unknown_class_in_context` | 1 | polymorph | recode: error.code = "E1027" after resolving |
| `E1028` | `dyn_incompatible_class` | 1 | polymorph | {} is not allowed: {reason}; use a generic f |
| `E2000` | `bindgen_annotation_error` | 1 | bindgen_driver | <<message>> |
| `E2000` | `bindgen_argument_error` | 1 | main | <<message>> |
| `E2000` | `cannot_derive_namespace` | 1 | package | cannot derive a namespace from the folder na |
| `E2000` | `debug_info_without_source_map` | 1 | llvm | debug information requires a source map |
| `E2000` | `duplicate_link_input` | 1 | driver | link input '{input}' specified more than onc |
| `E2000` | `duplicate_option`★ | 26 | main×26 | no-cache specified more than once |
| `E2000` | `environment_setting_invalid` | 5 | driver×5 | {SYSROOT_VARIABLE} does not apply to {}{} |
| `E2000` | `file_not_formatted` | 1 | main | hand-built JSON line: file is not formatted |
| `E2000` | `foreign_target_not_runnable` | 1 | driver | cannot run {name} {what} on this machine; ru |
| `E2000` | `formatter_changed_semantics` | 1 | formatter | formatter would change program semantics; pl |
| `E2000` | `formatter_invalid_output` | 1 | formatter | formatter produced invalid syntax: {}; pleas |
| `E2000` | `index_out_of_range` | 6 | llvm×2, main×2, test_runner×2 | test index is out of range; refresh the test |
| `E2000` | `input_count_invalid` | 1 | main | pass one .tz, .tt, or .tc file or project di |
| `E2000` | `input_extension_invalid`★ | 2 | driver×2 | input files must use '.tz' (code), '.tt' (ty |
| `E2000` | `invalid_library_name` | 1 | driver | <<message>> |
| `E2000` | `invalid_namespace_argument` | 1 | package | invalid namespace '{namespace}'; use identif |
| `E2000` | `invalid_publish_git_rev` | 1 | main | GIT_REV_RULE: the rev is not a full commit i |
| `E2000` | `invalid_publish_git_url` | 1 | main | GIT_URL_RULE: the git url is not an allowed  |
| `E2000` | `invalid_range_flag_setting` | 1 | llvm_ranges | {ENVIRONMENT_VARIABLE} must be off, proven o |
| `E2000` | `invalid_thread_setting` | 1 | parallel | {THREADS_VARIABLE} must be an integer from 1 |
| `E2000` | `invalid_trap_source_map` | 1 | trap | trap information requires a valid source map |
| `E2000` | `link_input_limit` | 1 | driver | too many link inputs; at most {MAX_LINK_INPU |
| `E2000` | `missing_input`★ | 1 | main | missing .tz, .tt, or .tc input or project di |
| `E2000` | `native_link_setting_not_allowed` | 3 | driver×2, fetch | a published package cannot declare [native]  |
| `E2000` | `net_async_needs_block_on` | 2 | driver, test_runner | <<crate::driver::NET_ASYNC_MESSA>> |
| `E2000` | `new_project_folder_not_empty` | 1 | package | '{}' is not empty; choose a new or empty fol |
| `E2000` | `new_project_io_failure` | 1 | package | cannot {action} '{}': {cause} |
| `E2000` | `no_sources_found` | 1 | driver | input directory has no .tz, .tt, or .tc sour |
| `E2000` | `option_conflict`★ | 22 | driver×15, llvm×3, main×3, test_runner | debugging a test needs exactly one --index |
| `E2000` | `option_invalid_for_command` | 25 | main×25 | lsp takes no paths or build options |
| `E2000` | `option_invalid_value` | 26 | driver×4, main×17, test_runner×5 | test optimization must be between 0 and 3 |
| `E2000` | `option_missing_value` | 3 | main×3 | --warn requires a warning name; supported: i |
| `E2000` | `option_requires_build`★ | 54 | driver×32, llvm×4, main×8, test_runner×10 | link inputs require a native executable; rem |
| `E2000` | `option_requires_option` | 6 | driver×4, main×2 | --wasm-feature net requires --wasm-feature j |
| `E2000` | `option_unsupported_platform` | 3 | driver×3 | '--emit shared' is not supported on Windows  |
| `E2000` | `output_extension_invalid` | 2 | bindgen_driver, driver | bindgen output must have the .tz extension |
| `E2000` | `project_manifest_missing` | 2 | main×2 | fetch requires a Tsuzuri.toml in the project |
| `E2000` | `repl_command_usage` | 4 | repl×4 | :type requires an expression |
| `E2000` | `repl_declaration_not_allowed` | 1 | repl | <<message>> |
| `E2000` | `repl_unknown_command` | 1 | repl | unknown command |
| `E2000` | `script_wrong_file_kind` | 1 | driver | a script is a .tz program; .tt and .tc files |
| `E2000` | `trap_info_without_source_map` | 1 | llvm_traps | trap information requires a source map |
| `E2000` | `unknown_explain_target` | 1 | explain（新規） | unknown diagnostic '{target}'; run 'tsuzuri  |
| `E2000` | `unknown_option`★ | 1 | main | unknown option '{value}'; use --help |
| `E2000` | `unknown_target` | 1 | main | parse_target(..) failure: unknown target '{v |
| `E2000` | `usage_error` | 7 | main×7 | usage: tsuzuri new directory [--namespace NA |
| `E2000` | `wasm_net_needs_opt_in` | 2 | driver, test_runner | <<crate::driver::NET_WASM_MESSAG>> |
| `E2000` | `wasm_os_api_needs_opt_in` | 2 | driver, test_runner | <<crate::driver::OS_WASM_MESSAGE>> |
| `E2000` | `wasm_test_runner_async` | 1 | test_runner | the WebAssembly test runner cannot drive Asy |
| `E2001` | `foreign_runner_not_startable` | 1 | test_runner | cannot start the runner of {} '{}': {error} |
| `E2001` | `input_not_regular_file` | 3 | bindgen_driver, driver×2 | the header must be a regular file |
| `E2001` | `input_not_utf8` | 1 | repl | cannot read the input: it is not valid UTF-8 |
| `E2001` | `input_path_not_utf8` | 1 | bindgen_driver | the header path must be valid UTF-8 |
| `E2001` | `input_too_large` | 1 | bindgen_driver | the header exceeds {MAX_AST_BYTES} bytes |
| `E2001` | `io_failure` | 2 | driver, fetch | cannot {action} '{}': {error} |
| `E2001` | `link_input_unreadable` | 1 | driver | cannot read link input '{}': not a {} |
| `E2001` | `repl_stream_failure` | 1 | main | cannot read or write the REPL's standard str |
| `E2001` | `temp_directory_failed` | 1 | driver | could not create a unique temporary director |
| `E2002` | `bindgen_unreadable_ast` | 1 | bindgen_driver | clang produced an unreadable AST ({error});  |
| `E2002` | `bindgen_unsupported_target` | 1 | bindgen_driver | bindgen supports only 64-bit LP64 targets (L |
| `E2002` | `external_tool_failed` | 2 | bindgen_driver, driver | '{name}' failed ({status}): {} {failure_hint |
| `E2002` | `external_tool_not_runnable` | 3 | bindgen_driver, driver, test_runner | cannot start {kind} runner: {error} |
| `E2002` | `external_tool_output_too_large` | 1 | bindgen_driver | {what} exceeds {MAX_AST_BYTES} bytes; bind a |
| `E2002` | `external_tool_read_failed` | 1 | bindgen_driver | cannot read from '{name}': {error} |
| `E2002` | `external_tool_wait_failed` | 4 | bindgen_driver, test_runner×3 | cannot wait for {kind} runner: {error} |
| `E2002` | `invalid_coverage_counters` | 1 | test_runner | invalid coverage counters in '{}': {message} |
| `E2002` | `platform_unsupported_api` | 10 | driver×4, test_runner×6 | <<crate::driver::os_message(plat>> |
| `E2002` | `toolchain_missing_feature` | 3 | driver, test_runner×2 | native parallel tasks require POSIX or Windo |
| `E2003` | `bindgen_output_not_generated` | 1 | bindgen_driver | refusing to overwrite '{}': it does not star |
| `E2003` | `doc_filename_collision` | 1 | docgen | documentation filenames collide, including t |
| `E2003` | `doc_output_changed` | 1 | driver | documentation output changed during generati |
| `E2003` | `doc_output_contains_sources` | 1 | driver | documentation output must not contain projec |
| `E2003` | `doc_output_invalid` | 2 | driver×2 | documentation output needs a directory name |
| `E2003` | `doc_output_not_owned` | 2 | driver×2 | existing documentation output needs a regula |
| `E2003` | `doc_publication_failed` | 1 | driver | documentation publication failed ({error}) a |
| `E2003` | `format_target_not_regular_file` | 1 | driver | fmt refuses symlinks, directories, and speci |
| `E2003` | `output_has_source_extension` | 1 | driver | an output must not have a '.tz', '.tt', or ' |
| `E2003` | `output_not_regular_file` | 2 | bindgen_driver, driver | the output must not be a symlink, directory, |
| `E2003` | `output_overwrites_input` | 2 | bindgen_driver, driver | bindgen output must not overwrite the header |
| `E2003` | `output_overwrites_link_input` | 1 | driver | output '{}' would overwrite a link input; ch |
| `E2003` | `output_publication_failed` | 1 | driver | output publication failed ({}) and a sidecar |
| `E2003` | `source_changed_during_formatting` | 2 | driver×2 | source changed during formatting; no replace |
| `E2004` | `ambiguous_entry_point` | 1 | check | Main.tz must use either top-level entry-poin |
| `E2004` | `builder_file_toplevel_code` | 1 | computation | a .tc file implements one builder named afte |
| `E2004` | `entry_point_missing`★ | 5 | driver×4, llvm | an executable requires top-level entry-point |
| `E2004` | `entry_point_result_type` | 1 | llvm | top-level entry-point code must end with IO< |
| `E2004` | `invalid_main_signature` | 1 | check | the entry point must be 'def main :: unit -> |
| `E2004` | `spirv_kernel_count` | 1 | driver | SPIR-V output requires exactly one exported  |
| `E2004` | `toplevel_code_outside_main` | 1 | check | top-level execution is only allowed in Main. |
| `E2005` | `benchmark_failed` | 1 | main | benchmark '{}.{}' {} |
| `E2005` | `program_exited_nonzero` | 1 | driver | <<message>> |
| `E2005` | `program_terminated` | 1 | driver | <<message>> |
| `E2005` | `program_time_limit_exceeded` | 1 | driver | <<message>> |
| `E2006` | `tests_failed` | 1 | main | {failed} test{} failed |
| `E2007` | `file_url_after_https` | 2 | fetch×2 | package '{}' was fetched over https and cann |
| `E2007` | `git_case_collision` | 1 | fetch | git dependency '{name}' has paths that diffe |
| `E2007` | `git_command_failed` | 3 | fetch×3 | git {command} of the registry index {} at {r |
| `E2007` | `git_missing_manifest` | 1 | fetch | git dependency '{name}' has no Tsuzuri.toml  |
| `E2007` | `git_non_regular_entry` | 1 | fetch | git dependency '{name}' has a symbolic link  |
| `E2007` | `git_source_not_utf8` | 1 | fetch | git dependency '{name}' has a source that is |
| `E2007` | `git_tree_unreadable` | 1 | fetch | git dependency '{name}' has an unreadable tr |
| `E2007` | `git_unsafe_path` | 2 | fetch×2 | git dependency '{name}' has an unsafe path ' |
| `E2007` | `git_version_unsupported` | 1 | fetch | git 2.32 or later is required to fetch git d |
| `E2007` | `invalid_lockfile` | 2 | driver, package | Tsuzuri.lock is not a valid lockfile ({detai |
| `E2007` | `lockfile_checksum_mismatch` | 3 | driver, fetch×2 | git dependency '{}' at rev {rev} has sha256  |
| `E2007` | `lockfile_entry_missing` | 2 | driver×2 | <<>> |
| `E2007` | `lockfile_missing` | 1 | driver | <<>> |
| `E2007` | `lockfile_not_regular_file` | 1 | driver | Tsuzuri.lock must be a regular file, not a s |
| `E2007` | `lockfile_version_mismatch` | 1 | driver | registry package '{}' declares version {} bu |
| `E2007` | `package_files_unreadable` | 1 | fetch | cannot read the package files of {} |
| `E2007` | `package_not_downloaded` | 1 | driver | <<>> |
| `E2007` | `package_store_unavailable` | 2 | driver, fetch | no package store is available; set TSUZURI_C |
| `E2007` | `package_store_unusable` | 1 | fetch | cannot use the cache directory {} for packag |
| `E2007` | `published_package_mismatch` | 1 | fetch | the package in {} does not match {url} at {r |
| `E2007` | `registry_checksum_mismatch` | 1 | fetch | registry package '{name}' {version} at {} ha |
| `E2007` | `registry_dependency_not_selected` | 1 | fetch | registry dependency '{}' was not selected; r |
| `E2007` | `registry_entry_mismatch` | 1 | fetch | registry package '{name}' {version} does not |
| `E2007` | `registry_index_invalid` | 1 | fetch | the registry index file for '{name}' is inva |
| `E2007` | `registry_index_missing` | 1 | fetch | registry dependency '{}' needs a [registry]  |
| `E2007` | `registry_package_missing` | 1 | fetch | the registry index has no package '{name}' |
| `E2007` | `registry_version_missing` | 1 | fetch | the registry index has no version {version}  |
| `W1001` | `unused_local`★ | 1 | warnings | unused local '{}'; prefix it with '_' to sil |
| `W1002` | `unused_private_function`★ | 1 | warnings | unused private function '{}'; remove it or m |
| `W1002` | `unused_private_type` | 1 | warnings | unused private type '{}'; remove it or make  |
| `W1003` | `unreachable_match_arm`★ | 1 | exhaustiveness | unreachable match arm; previous patterns alr |
| `W1004` | `shadowed_local`★ | 1 | check | local '{}' shadows an earlier binding in the |
| `W1005` | `deprecated_use`★ | 1 | warnings | <<used.message>> |
| `W1006` | `implicit_copy_of_large_value`★ | 1 | copies | <<message(&site.ty)>> |
| `W2001` | `tool_notice` | 4 | main×4 | hand-built JSON warning line (messages of th |
| `W2002` | `bindgen_skipped_declaration` | 1 | bindgen_driver | skipped C declaration '{}': {}; declare it b |
