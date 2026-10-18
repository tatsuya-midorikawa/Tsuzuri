# G23 付録: 発行箇所の表 — ツール・CLI・依存（driver、fetch、test_runner、bindgen、repl、main）

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

### `src/fetch.rs`（38 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | publish | loc | E1011 | invalid_publish_version | a published package needs [package] version in MAJOR.MINOR |
| 2 | publish | new | E1011 | publish_non_registry_dependency | a published package can depend only on registry packages;  |
| 3 | publish | new | E2000 | native_link_setting_not_allowed | a published package cannot declare [native] link settings |
| 4 | publish | loc | E2007 | package_files_unreadable | cannot read the package files of {} |
| 5 | publish | loc | E2007 | published_package_mismatch | the package in {} does not match {url} at {rev} (sha256 {l |
| 6 | Fetcher::resolve | man | E2007 | registry_dependency_not_selected | registry dependency '{}' was not selected; run tsuzuri fet |
| 7 | Fetcher::resolve_git | loc | E2007 | file_url_after_https | package '{}' was fetched over https and cannot use the fil |
| 8 | Fetcher::resolve_git | new | E2007 | lockfile_checksum_mismatch | git dependency '{}' at rev {rev} has sha256 {} but Tsuzuri |
| 9 | Fetcher::select | loc | E2007 | registry_index_missing | registry dependency '{}' needs a [registry] index in the r |
| 10 | Fetcher::select | loc | E2007 | registry_package_missing | the registry index has no package '{name}' |
| 11 | Fetcher::select | loc | E2007 | registry_version_missing | the registry index has no version {version} of '{name}' (r |
| 12 | Fetcher::select | loc | E1017 | registry_resolution_limit | registry resolution exceeds {MAX_REGISTRY_PACKAGES} packag |
| 13 | Fetcher::select | loc | E1011 | incompatible_dependency_versions | package '{}' is required at incompatible versions {} (by { |
| 14 | Fetcher::fetch_registry_package | loc | E2007 | file_url_after_https | the registry index was fetched over https and cannot use t |
| 15 | Fetcher::fetch_registry_package | new | E2007 | lockfile_checksum_mismatch | registry package '{name}' {version} has sha256 {} in the r |
| 16 | Fetcher::fetch_registry_package | loc | E2007 | registry_checksum_mismatch | registry package '{name}' {version} at {} has sha256 {} bu |
| 17 | Fetcher::fetch_registry_package | loc | E2007 | registry_entry_mismatch | registry package '{name}' {version} does not match its reg |
| 18 | Fetcher::store | loc | E2007 | package_store_unavailable | no package store is available; set TSUZURI_CACHE_DIR and r |
| 19 | Fetcher::store | new | E2007 | package_store_unusable | cannot use the cache directory {} for packages ({detail}); |
| 20 | Fetcher::check_git | loc | E2007 | git_version_unsupported | git 2.32 or later is required to fetch git dependencies (f |
| 21 | Index::open | loc | E2007 | git_command_failed | git {command} of the registry index {} at {rev} failed: {d |
| 22 | Index::open | loc | E1017 | registry_index_too_large | the registry index has a listing larger than 64 MiB |
| 23 | Index::load | loc | E1017 | registry_index_too_large | the registry index file for '{name}' exceeds 1 MiB |
| 24 | Index::load | loc | E2007 | git_command_failed | git cat-file of the registry index {} failed: {detail} |
| 25 | Index::load | loc | E2007 | registry_index_invalid | the registry index file for '{name}' is invalid ({detail}) |
| 26 | io_error | new | E2001 | io_failure | cannot {action} '{}': {error} |
| 27 | parse_tree | man | E2007 | git_tree_unreadable | git dependency '{name}' has an unreadable tree listing |
| 28 | parse_tree | man | E1017 | module_discovery_limit | module discovery exceeds 1024 directories or 16 path segme |
| 29 | parse_tree | man | E2007 | git_unsafe_path | git dependency '{name}' has an unsafe path '{}' |
| 30 | parse_tree | man | E2007 | git_unsafe_path | git dependency '{name}' has an unsafe path '{path}' |
| 31 | parse_tree | man | E2007 | git_non_regular_entry | git dependency '{name}' has a symbolic link or submodule a |
| 32 | parse_tree | man | E0003 | source_too_large | source exceeds the {MAX_SOURCE_BYTES}-byte limit |
| 33 | parse_tree | man | E1017 | module_discovery_limit | module discovery exceeds 4096 source files |
| 34 | parse_tree | man | E2007 | git_case_collision | git dependency '{name}' has paths that differ only in case |
| 35 | parse_tree | man | E2007 | git_missing_manifest | git dependency '{name}' has no Tsuzuri.toml at the reposit |
| 36 | download | loc | E2007 | git_command_failed | git {command} of {url} at {rev} failed: {detail}; check th |
| 37 | download | loc | E1017 | git_dependency_too_large | git dependency '{name}' has a tree listing larger than 64  |
| 38 | download | loc | E2007 | git_source_not_utf8 | git dependency '{name}' has a source that is not UTF-8: '{ |

### `src/driver.rs`（146 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | LinkInputs::check_shape | drv | E2000 | link_input_limit | too many link inputs; at most {MAX_LINK_INPUTS} are suppor |
| 2 | LinkInputs::check_shape | drv | E2000 | invalid_library_name | <<message>> |
| 3 | LinkInputs::check_shape | drv | E2000 | duplicate_link_input | link input '{input}' specified more than once |
| 4 | LinkInputs::check_readable | drv | E2001 | link_input_unreadable | cannot read link input '{}': not a {} |
| 5 | protect_links | drv | E2003 | output_overwrites_link_input | output '{}' would overwrite a link input; choose a differe |
| 6 | BuildOptions::validate | drv | E2000 | option_requires_build | SPIR-V output does not use target, CPU, debug, or WASM fea |
| 7 | BuildOptions::validate | drv | E2000 | option_requires_build | --wasm-feature threads requires wasm32 object or WASM outp |
| 8 | BuildOptions::validate | drv | E2000 | option_requires_build | --wasm-feature jspi requires wasm32 or wasm64 object, LLVM |
| 9 | BuildOptions::validate | drv | E2000 | option_conflict | --wasm-feature jspi cannot be combined with --wasm-feature |
| 10 | BuildOptions::validate | drv | E2000 | option_requires_build | --wasm-feature webgpu requires wasm32 object, LLVM IR, or  |
| 11 | BuildOptions::validate | drv | E2000 | option_conflict | --wasm-feature webgpu cannot be combined with --wasm-featu |
| 12 | BuildOptions::validate | drv | E2000 | option_requires_build | --wasm-feature net requires wasm32 object, LLVM IR, WASM,  |
| 13 | BuildOptions::validate | drv | E2000 | option_requires_option | --wasm-feature net requires --wasm-feature jspi: a blockin |
| 14 | BuildOptions::validate | drv | E2000 | option_conflict | --wasm-feature net cannot be combined with --wasm-feature  |
| 15 | BuildOptions::validate | drv | E2000 | option_requires_build | --wasm-feature simd128 requires wasm32 or wasm64 object, L |
| 16 | BuildOptions::validate | drv | E2000 | option_requires_build | --wasm-host wasi requires wasm32 object or WASM output |
| 17 | BuildOptions::validate | drv | E2000 | option_conflict | --wasm-host wasi cannot be combined with --wasm-feature th |
| 18 | BuildOptions::validate | drv | E2000 | option_requires_build | --debug-info is not valid for header output |
| 19 | BuildOptions::validate | drv | E2000 | option_requires_build | --trap-info is not valid for header output |
| 20 | BuildOptions::validate | drv | E2000 | option_requires_build | --debug-output is not valid for header output |
| 21 | BuildOptions::validate | drv | E2000 | option_requires_build | --trap-mode return is only valid for native object, llvm o |
| 22 | BuildOptions::validate | drv | E2000 | option_requires_build | --strip requires a native executable; WASM output is alrea |
| 23 | BuildOptions::validate | drv | E2000 | option_requires_build | --strip requires native executable output (--emit exe) |
| 24 | BuildOptions::validate | drv | E2000 | option_conflict | --strip cannot be combined with --debug-info: a debugger n |
| 25 | BuildOptions::validate | drv | E2000 | option_requires_build | --wasm-opt requires WASM output (--target wasm32, --emit w |
| 26 | BuildOptions::validate | drv | E2000 | option_conflict | --wasm-opt cannot be combined with --wasm-feature threads |
| 27 | BuildOptions::validate | drv | E2000 | option_conflict | --wasm-opt cannot be combined with --debug-info: the optim |
| 28 | BuildOptions::validate | drv | E2000 | option_requires_build | '--emit exe' requires '--target native'; '--emit wasm' req |
| 29 | BuildOptions::validate | drv | E2000 | option_requires_build | '--cpu native' requires native executable or object output |
| 30 | BuildOptions::validate | drv | E2000 | option_requires_build | '--cpu native' requires the host target; use '--cpu generi |
| 31 | BuildOptions::validate | drv | E2000 | option_requires_build | --wasm-max-memory requires wasm32 or wasm64 object, LLVM I |
| 32 | BuildOptions::validate | drv | E2000 | option_requires_build | --wasm-stack-size requires WASM output; link object and LL |
| 33 | BuildOptions::validate_bindings | drv | E2000 | option_requires_build | '--emit {kind}' requires '--target native' |
| 34 | BuildOptions::validate_bindings | drv | E2000 | option_requires_build | {option} is not valid for bindings output; pass it when bu |
| 35 | BuildOptions::validate_bindings | drv | E2000 | option_requires_build | '--emit bindings-js' requires '--target wasm32' |
| 36 | BuildOptions::validate_bindings | drv | E2000 | option_requires_build | {option} is not valid for bindings output; pass it when bu |
| 37 | BuildOptions::validate_shared | drv | E2000 | option_requires_build | '--emit shared' requires '--target native' |
| 38 | BuildOptions::validate_shared | drv | E2000 | option_unsupported_platform | '--emit shared' is not supported on Windows yet (G10); bui |
| 39 | BuildOptions::validate_shared | drv | E2000 | option_conflict | --allocator host cannot be combined with --emit shared: a  |
| 40 | BuildOptions::validate_triple | drv | E2000 | option_invalid_value | --target {name} names a native target, not WebAssembly |
| 41 | BuildOptions::validate_triple | drv | E2000 | option_requires_option | {name} has no operating system: build it with --freestandi |
| 42 | BuildOptions::validate_triple | drv | E2000 | option_unsupported_platform | --debug-info for {name} needs a {needed} host: {tool} |
| 43 | BuildOptions::validate_allocator | drv | E2000 | option_conflict | --allocator host cannot be combined with --wasm-feature th |
| 44 | BuildOptions::validate_allocator | drv | E2000 | option_requires_build | --allocator host requires object, LLVM IR, header, or WebA |
| 45 | BuildOptions::validate_allocator | drv | E2000 | option_requires_build | --allocator counting requires object, LLVM IR, header, or  |
| 46 | BuildOptions::validate_allocator | drv | E2000 | option_conflict | --trap-mode return keeps its own tracked allocator; remove |
| 47 | BuildOptions::validate_allocator | drv | E2000 | option_requires_build | --allocator small requires a native target; WebAssembly mo |
| 48 | BuildOptions::validate_allocator | drv | E2000 | option_requires_build | --allocator small requires executable, object, shared libr |
| 49 | BuildOptions::validate_allocator | drv | E2000 | option_requires_build | --freestanding requires a native target |
| 50 | BuildOptions::validate_allocator | drv | E2000 | option_requires_build | --freestanding requires object, LLVM IR, or header output |
| 51 | BuildOptions::validate_allocator | drv | E2000 | option_requires_option | --freestanding requires --allocator host: without the C li |
| 52 | BuildOptions::validate_allocator | drv | E2000 | option_conflict | --freestanding cannot be combined with --trap-info or --de |
| 53 | bindings_output_error | drv | E2000 | output_extension_invalid | bindings output must end with '.mjs'; declarations are wri |
| 54 | wasm_memory_limits | drv | E2000 | option_invalid_value | --wasm-stack-size must be a multiple of 16 bytes and at le |
| 55 | wasm_memory_limits | drv | E2000 | option_invalid_value | <<message>> |
| 56 | wasm_memory_limits | drv | E2000 | option_invalid_value | --wasm-max-memory must be at least the stack size plus 64  |
| 57 | native_cpu_flag | drv | E2000 | option_unsupported_platform | '--cpu native' is not supported on {architecture}; use '-- |
| 58 | sdk_args_for | drv | E2000 | environment_setting_invalid | {SYSROOT_VARIABLE} does not apply to {}{} |
| 59 | final_link_args_for | drv | E2000 | environment_setting_invalid | {LINKER_VARIABLE} must be 'default' or 'lld' |
| 60 | final_link_args_for | drv | E2000 | environment_setting_invalid | {LINKER_VARIABLE}=lld is not supported for Windows targets |
| 61 | final_link_args_for | drv | E2000 | environment_setting_invalid | {LINKER_VARIABLE} must be 'default' or 'lld', not '{other} |
| 62 | check_linker_with_optimization | drv | E2000 | environment_setting_invalid | {LINKER_VARIABLE}=lld cannot be combined with {} for a Mac |
| 63 | SourceFile::read | drv | E1011 | invalid_module_filename | the module filename must be an ASCII identifier |
| 64 | collect_sources | drv | E1017 | module_discovery_limit | module discovery exceeds 1024 directories or 16 path segme |
| 65 | collect_sources | drv | E1011 | symlink_not_followed | module discovery does not follow source or directory symbo |
| 66 | collect_sources | drv | E1011 | module_source_not_regular_file | module sources must be regular files |
| 67 | collect_sources | drv | E1017 | module_discovery_limit | module discovery exceeds 4096 source files |
| 68 | read_manifest | drv | E1011 | manifest_not_regular_file | package manifests must be regular files, not symbolic link |
| 69 | load_packages | drv | E1011 | cyclic_package_dependency | cyclic package dependency |
| 70 | load_packages | drv | E1017 | package_graph_limit | package graph exceeds 1024 packages or depth 128 |
| 71 | load_packages | drv | E1011 | reserved_package_namespace | package namespace is reserved by the standard library |
| 72 | load_packages | drv | E1011 | ambiguous_package_root | package name or namespace resolves to multiple roots |
| 73 | load_packages | drv | E2007 | lockfile_version_mismatch | registry package '{}' declares version {} but Tsuzuri.lock |
| 74 | load_packages | clo | E1011 | conflicting_dependency_source | package '{name}' is required from different sources; a pac |
| 75 | load_packages | clo | E1011 | dependency_kind_not_allowed | registry packages can depend only on registry packages; us |
| 76 | load_packages | clo | E1011 | dependency_kind_not_allowed | git packages cannot have path dependencies; use a git depe |
| 77 | load_packages | new | E1011 | dependency_name_mismatch | dependency key must match the dependency package name |
| 78 | dependency_directory | new | E1011 | dependency_path_not_relative | dependency paths must be relative |
| 79 | dependency_directory | new | E1011 | symlink_not_followed | dependency paths must traverse real directories, not symbo |
| 80 | read_lockfile | drv | E2007 | lockfile_not_regular_file | Tsuzuri.lock must be a regular file, not a symbolic link o |
| 81 | read_lockfile | drv | E1017 | lockfile_too_large | Tsuzuri.lock exceeds 1 MiB |
| 82 | read_lockfile | drv | E2007 | invalid_lockfile | Tsuzuri.lock is not a valid lockfile (it is not UTF-8); re |
| 83 | offline_package | clo | E2007 | lockfile_missing | <<>> |
| 84 | offline_package | clo | E2007 | lockfile_entry_missing | <<>> |
| 85 | offline_package | clo | E2007 | lockfile_entry_missing | <<>> |
| 86 | offline_package | clo | E2007 | package_store_unavailable | <<>> |
| 87 | offline_package | clo | E2007 | package_not_downloaded | <<>> |
| 88 | Project::load_with_overlays | drv | E0003 | source_too_large | source exceeds the {MAX_SOURCE_BYTES}-byte limit |
| 89 | Project::load_with_overlays | drv | E1011 | source_without_filename | a source file needs a filename |
| 90 | Project::load_from_root | drv | E1011 | source_outside_package_root | source is outside its package root |
| 91 | Project::load_from_root | drv | E1011 | module_namespace_collision | root module collides with a dependency namespace |
| 92 | Project::load_from_root | drv | E1017 | package_graph_limit | package graph exceeds 4096 source files |
| 93 | Project::load_from_root | drv | E2007 | lockfile_checksum_mismatch | {} '{}' does not match its sha256 in Tsuzuri.lock; run tsu |
| 94 | Project::load_from_root | drv | E1011 | source_filename_case_mismatch | source filename must match its directory entry exactly, in |
| 95 | Project::load_from_root | new | E2000 | native_link_setting_not_allowed | git and registry packages cannot declare [native] link set |
| 96 | Project::load_from_root | new | E2000 | native_link_setting_not_allowed | only the root package and dependencies it marks with nativ |
| 97 | Project::load_without_entry | drv | E2000 | no_sources_found | input directory has no .tz, .tt, or .tc sources |
| 98 | Project::load_with | drv | E1011 | symlink_not_followed | source inputs must not be symbolic links |
| 99 | Project::load_script | drv | E2000 | script_wrong_file_kind | a script is a .tz program; .tt and .tc files are type clas |
| 100 | document | drv | E2003 | doc_output_invalid | documentation output needs a directory name |
| 101 | document | drv | E2003 | doc_output_changed | documentation output changed during generation |
| 102 | document | drv | E2003 | doc_publication_failed | documentation publication failed ({error}) and restore fai |
| 103 | protect_documentation | drv | E2003 | doc_output_invalid | documentation output must be a regular directory, not a sy |
| 104 | protect_documentation | drv | E2003 | doc_output_not_owned | existing documentation output needs a regular .tsuzuri-doc |
| 105 | protect_documentation | drv | E2003 | doc_output_not_owned | documentation marker does not identify tsuzuri doc output |
| 106 | protect_documentation | drv | E2003 | doc_output_contains_sources | documentation output must not contain project sources |
| 107 | read_source | drv | E2000 | input_extension_invalid | input files must use '.tz' (code), '.tt' (type classes), o |
| 108 | read_source_text | drv | E2001 | input_not_regular_file | the source must be a regular file |
| 109 | read_source_text | drv | E2001 | input_not_regular_file | the source must be a regular file |
| 110 | read_source_text | drv | E0003 | source_too_large | source exceeds the {MAX_SOURCE_BYTES}-byte limit |
| 111 | format_sources | drv | E2000 | input_extension_invalid | fmt accepts .tz, .tt, or .tc files |
| 112 | require_regular_source | drv | E2003 | format_target_not_regular_file | fmt refuses symlinks, directories, and special files |
| 113 | replace_source_atomically | drv | E2003 | source_changed_during_formatting | source changed during formatting; no replacement made |
| 114 | replace_source_atomically | drv | E2003 | source_changed_during_formatting | source changed during formatting; no replacement made |
| 115 | build_complete | drv | E2000 | option_requires_build | link inputs require a native executable; remove --link, -l |
| 116 | build_complete | drv | E2000 | option_conflict | --trap-mode return cannot be combined with Async.start or  |
| 117 | build_complete | drv | E2000 | option_conflict | --wasm-feature threads cannot be combined with Async.start |
| 118 | build_complete | drv | E2000 | option_requires_option | Async.block_on on WebAssembly needs --wasm-feature jspi: i |
| 119 | build_complete | drv | E2004 | entry_point_missing | an application must start from Main.tz; pass Main.tz or it |
| 120 | build_complete | drv | E2004 | entry_point_missing | a WebAssembly module needs 'def main', top-level IO<T> ent |
| 121 | build_complete | drv | E2004 | entry_point_missing | a shared library needs at least one 'export def' entry poi |
| 122 | build_complete | drv | E2004 | spirv_kernel_count | SPIR-V output requires exactly one exported scalar kernel; |
| 123 | build_complete | drv | E2000 | option_conflict | --trap-mode return cannot be combined with extern callback |
| 124 | build_complete | drv | E2000 | option_conflict | --freestanding cannot use {feature}: its runtime needs the |
| 125 | build_complete | drv | E2002 | toolchain_missing_feature | native parallel tasks require a POSIX or Windows toolchain |
| 126 | build_complete | drv | E2000 | wasm_os_api_needs_opt_in | <<OS_WASM_MESSAGE>> |
| 127 | build_complete | drv | E2000 | wasm_net_needs_opt_in | <<NET_WASM_MESSAGE>> |
| 128 | build_complete | drv | E2000 | net_async_needs_block_on | <<NET_ASYNC_MESSAGE>> |
| 129 | build_complete | drv | E2002 | platform_unsupported_api | <<net_message(platform)>> |
| 130 | build_complete | drv | E2002 | platform_unsupported_api | <<async_message(platform)>> |
| 131 | build_complete | drv | E2002 | platform_unsupported_api | <<os_message(platform)>> |
| 132 | build_complete | drv | E2002 | platform_unsupported_api | Windows COFF objects with embedded task, CPU, IO, or Async |
| 133 | build_bindings | drv | E2004 | entry_point_missing | bindings need at least one 'export def' entry point |
| 134 | publish_outputs | drv | E2003 | output_publication_failed | output publication failed ({}) and a sidecar could not be  |
| 135 | run_process | drv | E2000 | option_requires_build | run requires native executable output |
| 136 | run_process | drv | E2005 | program_exited_nonzero | <<message>> |
| 137 | run_process | new | E2005 | program_terminated | <<message>> |
| 138 | capture | new | E2005 | program_time_limit_exceeded | <<message>> |
| 139 | foreign_runner | drv | E2000 | foreign_target_not_runnable | cannot run {name} {what} on this machine; run {them} on a  |
| 140 | run_tool | drv | E2002 | external_tool_not_runnable | cannot execute '{name}': {error}; {hint} |
| 141 | run_tool | drv | E2002 | external_tool_failed | '{name}' failed ({}): {} {hint} |
| 142 | protect_source | drv | E2003 | output_has_source_extension | an output must not have a '.tz', '.tt', or '.tc' source ex |
| 143 | protect_source | drv | E2003 | output_not_regular_file | the output must not be a symlink, directory, or special fi |
| 144 | protect_source | drv | E2003 | output_overwrites_input | refusing to overwrite the input source |
| 145 | io_error | drv | E2001 | io_failure | cannot {action} '{}': {error} |
| 146 | TemporaryDirectory::new | drv | E2001 | temp_directory_failed | could not create a unique temporary directory |

### `src/test_runner.rs`（36 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | run_with_timeout | drv | E2000 | option_invalid_value | test optimization must be between 0 and 3 |
| 2 | run_with_timeout | drv | E2000 | option_requires_build | link inputs require a native executable; remove --link, -l |
| 3 | run_with_timeout | drv | E2000 | option_requires_build | --wasm-max-memory and --wasm-stack-size require --target w |
| 4 | run_with_timeout | drv | E2000 | option_requires_build | test coverage supports only the native target |
| 5 | run_with_timeout | drv | E2000 | option_invalid_value | --target {} names a native target, not WebAssembly |
| 6 | run_with_timeout | drv | E2000 | option_requires_build | {} has no operating system: a test needs an executable, wh |
| 7 | run_with_timeout | drv | E2000 | option_requires_build | test coverage needs a target this machine runs; run 'tsuzu |
| 8 | merge_coverage | drv | E2002 | invalid_coverage_counters | invalid coverage counters in '{}': {message} |
| 9 | build_debug_runner | drv | E2000 | option_conflict | debugging a test needs exactly one --index |
| 10 | build_debug_runner | drv | E2000 | index_out_of_range | test index is out of range; refresh the test list |
| 11 | build_debug_runner | drv | E2000 | option_requires_build | debugging a test requires the native target |
| 12 | build_debug_runner | drv | E2000 | option_requires_build | debugging a test needs a target this machine runs; use 'ts |
| 13 | build_debug_runner | drv | E2000 | option_invalid_value | test optimization must be between 0 and 3 |
| 14 | native_runtime_sources | drv | E2002 | toolchain_missing_feature | native parallel tasks require POSIX or Windows threads |
| 15 | native_runtime_sources | drv | E2002 | platform_unsupported_api | <<crate::driver::os_message(plat>> |
| 16 | native_runtime_sources | drv | E2002 | platform_unsupported_api | <<crate::driver::net_message(pla>> |
| 17 | native_runtime_sources | drv | E2000 | net_async_needs_block_on | <<crate::driver::NET_ASYNC_MESSA>> |
| 18 | native_runtime_sources | drv | E2002 | platform_unsupported_api | <<crate::driver::async_message(p>> |
| 19 | build_runner | drv | E2000 | wasm_os_api_needs_opt_in | <<crate::driver::OS_WASM_MESSAGE>> |
| 20 | build_runner | drv | E2000 | wasm_net_needs_opt_in | <<crate::driver::NET_WASM_MESSAG>> |
| 21 | build_runner | drv | E2000 | wasm_test_runner_async | the WebAssembly test runner cannot drive Async.start or As |
| 22 | compile_native_runner | drv | E2002 | toolchain_missing_feature | native parallel tasks require POSIX or Windows threads |
| 23 | compile_native_runner | drv | E2002 | platform_unsupported_api | <<crate::driver::os_message(plat>> |
| 24 | compile_native_runner | drv | E2002 | platform_unsupported_api | <<crate::driver::net_message(pla>> |
| 25 | compile_native_runner | drv | E2002 | platform_unsupported_api | <<crate::driver::async_message(p>> |
| 26 | run_benches_linked | drv | E2000 | option_requires_build | tsuzuri bench supports only the native target |
| 27 | run_benches_linked | drv | E2000 | option_requires_build | tsuzuri bench measures the machine it runs on and cannot r |
| 28 | run_benches_linked | drv | E2000 | option_requires_build | a bare-metal target has no operating system to run a bench |
| 29 | run_benches_linked | drv | E2000 | option_invalid_value | bench optimization must be between 0 and 3 |
| 30 | run_benches_linked | drv | E2000 | option_invalid_value | bench samples must be an integer between 1 and 1000 |
| 31 | run_benches_linked | drv | E2000 | index_out_of_range | bench index is out of range; refresh the bench list |
| 32 | run_captured | drv | E2001 | foreign_runner_not_startable | cannot start the runner of {} '{}': {error} |
| 33 | run_captured | drv | E2002 | external_tool_not_runnable | cannot start {kind} runner: {error} |
| 34 | run_captured | drv | E2002 | external_tool_wait_failed | cannot wait for {kind} runner: {error} |
| 35 | run_captured | drv | E2002 | external_tool_wait_failed | cannot collect timed-out {kind}: {error} |
| 36 | run_captured | drv | E2002 | external_tool_wait_failed | cannot stop timed-out {kind}: {error} |

### `src/bindgen_driver.rs`（16 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | bindgen | drv | E2001 | input_not_regular_file | the header must be a regular file |
| 2 | bindgen | drv | E2001 | input_too_large | the header exceeds {MAX_AST_BYTES} bytes |
| 3 | bindgen | drv | E2001 | input_path_not_utf8 | the header path must be valid UTF-8 |
| 4 | bindgen | drv | E2002 | bindgen_unsupported_target | bindgen supports only 64-bit LP64 targets (Linux and macOS |
| 5 | bindgen | drv | E2002 | bindgen_unreadable_ast | clang produced an unreadable AST ({error}); use LLVM/Clang |
| 6 | bindgen | drv | E2000 | bindgen_annotation_error | <<message>> |
| 7 | bindgen | warn | W2002 | bindgen_skipped_declaration | skipped C declaration '{}': {}; declare it by hand or wrap |
| 8 | capture | drv | E2002 | external_tool_not_runnable | cannot execute '{name}': {error}; {CLANG_HINT} |
| 9 | capture | drv | E2002 | external_tool_output_too_large | {what} exceeds {MAX_AST_BYTES} bytes; bind a smaller heade |
| 10 | capture | drv | E2002 | external_tool_wait_failed | cannot wait for '{name}': {error} |
| 11 | capture | drv | E2002 | external_tool_read_failed | cannot read from '{name}': {error} |
| 12 | capture | drv | E2002 | external_tool_failed | '{name}' failed ({status}): {} {failure_hint} |
| 13 | protect_bindgen_output | drv | E2000 | output_extension_invalid | bindgen output must have the .tz extension |
| 14 | protect_bindgen_output | drv | E2003 | output_not_regular_file | the output must not be a symlink, directory, or special fi |
| 15 | protect_bindgen_output | drv | E2003 | output_overwrites_input | bindgen output must not overwrite the header |
| 16 | protect_bindgen_output | drv | E2003 | bindgen_output_not_generated | refusing to overwrite '{}': it does not start with the tsu |

### `src/repl.rs`（9 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | too_large | new | E0003 | source_too_large | source exceeds the {MAX_SOURCE_BYTES}-byte limit; split th |
| 2 | invalid_utf8 | new | E2001 | input_not_utf8 | cannot read the input: it is not valid UTF-8 |
| 3 | classify | new | E2000 | repl_declaration_not_allowed | <<message>> |
| 4 | Repl::command | man | E2000 | repl_command_usage | NO_ARGUMENTS (a command that takes no arguments was given  |
| 5 | Repl::command | man | E2000 | repl_command_usage | :type requires an expression |
| 6 | Repl::command | man | E2000 | repl_command_usage | :load requires one .tz file path |
| 7 | Repl::command | man | E2000 | repl_unknown_command | unknown command |
| 8 | Repl::type_of | new | E2000 | repl_command_usage | :type requires an expression |
| 9 | Repl::check | new | E0003 | source_too_large | source exceeds the {MAX_SOURCE_BYTES}-byte limit; split th |

### `src/main.rs`（110 行）

| # | 関数 | 形 | コード | kind | メッセージ |
| --- | --- | --- | --- | --- | --- |
| 1 | parse_arguments | man | E2000 | option_invalid_for_command | lsp takes no paths or build options |
| 2 | parse_arguments | man | E2000 | duplicate_option | no-cache specified more than once |
| 3 | parse_arguments | man | E2000 | duplicate_option | deny-warnings specified more than once |
| 4 | parse_arguments | man | E2000 | option_missing_value | --warn requires a warning name; supported: implicit-copy |
| 5 | parse_arguments | man | E2000 | option_invalid_value | unknown warning '{}' for --warn; supported: implicit-copy |
| 6 | parse_arguments | man | E2000 | duplicate_option | warn implicit-copy specified more than once |
| 7 | parse_arguments | man | E2000 | duplicate_option | check specified more than once |
| 8 | parse_arguments | man | E2000 | duplicate_option | list specified more than once |
| 9 | parse_arguments | man | E2000 | option_invalid_value | test index must be a nonnegative integer |
| 10 | parse_arguments | man | E2000 | duplicate_option | filter specified more than once |
| 11 | parse_arguments | man | E2000 | option_invalid_value | test filter must be UTF-8 |
| 12 | parse_arguments | man | E2000 | duplicate_option | samples specified more than once |
| 13 | parse_arguments | man | E2000 | option_invalid_value | bench samples must be an integer between 1 and 1000 |
| 14 | parse_arguments | man | E2000 | duplicate_option | seed specified more than once |
| 15 | parse_arguments | man | E2000 | option_invalid_value | property seed must be an integer between 0 and 18446744073 |
| 16 | parse_arguments | man | E2000 | duplicate_option | coverage specified more than once |
| 17 | parse_arguments | man | E2000 | duplicate_option | debug-output specified more than once |
| 18 | parse_arguments | man | E2000 | duplicate_option | trap-info specified more than once |
| 19 | parse_arguments | man | E2000 | duplicate_option | trap mode specified more than once |
| 20 | parse_arguments | man | E2000 | option_invalid_value | trap mode must be 'return' |
| 21 | parse_arguments | man | E2000 | duplicate_option | allocator specified more than once |
| 22 | parse_arguments | man | E2000 | option_invalid_value | allocator must be 'system', 'host', 'counting', or 'small' |
| 23 | parse_arguments | man | E2000 | duplicate_option | freestanding specified more than once |
| 24 | parse_arguments | man | E2000 | duplicate_option | strip specified more than once |
| 25 | parse_arguments | man | E2000 | duplicate_option | wasm-opt specified more than once |
| 26 | parse_arguments | man | E2000 | duplicate_option | debug information specified more than once |
| 27 | parse_arguments | man | E2000 | duplicate_option | output specified more than once |
| 28 | parse_arguments | man | E2000 | duplicate_option | target specified more than once |
| 29 | parse_arguments | man | E2000 | option_invalid_value | target names are UTF-8; use native, wasm32, wasm64, or a T |
| 30 | parse_arguments | man | E2000 | unknown_target | parse_target(..) failure: unknown target '{value}' |
| 31 | parse_arguments | man | E2000 | option_invalid_value | supported WASM features are 'simd128', 'threads', 'jspi',  |
| 32 | parse_arguments | man | E2000 | duplicate_option | WASM feature specified more than once |
| 33 | parse_arguments | man | E2000 | duplicate_option | --wasm-host specified more than once |
| 34 | parse_arguments | man | E2000 | option_invalid_value | unknown wasm host '{}'; the supported host is wasi |
| 35 | parse_arguments | man | E2000 | duplicate_option | WASM {name} specified more than once |
| 36 | parse_arguments | man | E2000 | duplicate_option | emit kind specified more than once |
| 37 | parse_arguments | man | E2000 | option_invalid_value | emit kind must be exe, object, llvm, header, wasm, ... |
| 38 | parse_arguments | man | E2000 | duplicate_option | optimization specified more than once |
| 39 | parse_arguments | man | E2000 | duplicate_option | CPU tuning specified more than once |
| 40 | parse_arguments | man | E2000 | option_invalid_value | CPU tuning must be 'generic' or 'native' |
| 41 | parse_arguments | man | E2000 | option_invalid_value | library names must be UTF-8 |
| 42 | parse_arguments | man | E2000 | unknown_option | unknown option '{value}'; use --help |
| 43 | parse_arguments | man | E2000 | input_count_invalid | pass one .tz, .tt, or .tc file or project directory; modul |
| 44 | parse_arguments | man | E2000 | missing_input | missing .tz, .tt, or .tc input or project directory; use - |
| 45 | parse_arguments | man | E2000 | option_invalid_for_command | --no-cache is only valid with build, run, script, or repl |
| 46 | parse_arguments | man | E2000 | option_requires_build | SPIR-V output does not use target, optimization, or CPU op |
| 47 | parse_arguments | man | E2000 | option_invalid_for_command | --wasm-feature is only valid with build |
| 48 | parse_arguments | man | E2000 | option_invalid_for_command | --wasm-host is only valid with build |
| 49 | parse_arguments | man | E2000 | option_invalid_for_command | --wasm-max-memory and --wasm-stack-size are only valid wit |
| 50 | parse_arguments | man | E2000 | option_requires_build | --wasm-max-memory and --wasm-stack-size require --target w |
| 51 | parse_arguments | man | E2000 | option_invalid_for_command | --debug-info is only valid with build, run, or test |
| 52 | parse_arguments | man | E2000 | option_requires_option | debugging a test needs both -g and -o with the runner's pa |
| 53 | parse_arguments | man | E2000 | option_conflict | debugging a test needs exactly one --index, without --list |
| 54 | parse_arguments | man | E2000 | option_requires_build | debugging a test requires the native target |
| 55 | parse_arguments | man | E2000 | option_invalid_for_command | --debug-output is only valid with build or run |
| 56 | parse_arguments | man | E2000 | option_invalid_for_command | --trap-info is only valid with build or run |
| 57 | parse_arguments | man | E2000 | option_invalid_for_command | --trap-mode is only valid with build |
| 58 | parse_arguments | man | E2000 | option_invalid_for_command | --allocator is only valid with build |
| 59 | parse_arguments | man | E2000 | option_invalid_for_command | --freestanding is only valid with build |
| 60 | parse_arguments | man | E2000 | option_invalid_for_command | --strip is only valid with build |
| 61 | parse_arguments | man | E2000 | option_invalid_for_command | --wasm-opt is only valid with build |
| 62 | parse_arguments | man | E2000 | option_invalid_for_command | --check is only valid with fmt |
| 63 | parse_arguments | man | E2000 | option_invalid_for_command | fmt does not use optimization, CPU tuning, or compiler war |
| 64 | parse_arguments | man | E2000 | option_invalid_value | {name} supports -O0 to -O3; use -Os or -Oz with build or r |
| 65 | parse_arguments | man | E2000 | option_invalid_for_command | --filter, --list, and --index are only valid with test or  |
| 66 | parse_arguments | man | E2000 | option_invalid_for_command | test does not use CPU tuning |
| 67 | parse_arguments | man | E2000 | option_invalid_for_command | --samples is only valid with bench |
| 68 | parse_arguments | man | E2000 | option_invalid_for_command | --seed is only valid with test |
| 69 | parse_arguments | man | E2000 | option_invalid_for_command | bench does not use CPU tuning |
| 70 | parse_arguments | man | E2000 | option_requires_build | tsuzuri bench supports only the native target |
| 71 | parse_arguments | man | E2000 | option_invalid_for_command | --coverage is only valid with test |
| 72 | parse_arguments | man | E2000 | option_requires_build | test coverage supports only the native target |
| 73 | parse_arguments | man | E2000 | option_conflict | coverage cannot be combined with --list |
| 74 | parse_arguments | man | E2000 | option_conflict | coverage cannot be combined with -g; a debug runner only b |
| 75 | parse_arguments | man | E2000 | option_requires_build | run supports only a native target |
| 76 | parse_arguments | man | E2000 | option_invalid_for_command | --output requires build or doc; --target and --emit requir |
| 77 | parse_arguments | man | E2000 | option_requires_option | doc requires -o or --output with an output directory |
| 78 | parse_arguments | man | E2000 | option_invalid_for_command | doc does not use optimization or CPU tuning |
| 79 | parse_arguments | man | E2000 | option_invalid_for_command | check does not use an optimization level |
| 80 | parse_arguments | man | E2000 | option_invalid_for_command | check does not use CPU tuning |
| 81 | parse_arguments | man | E2000 | option_requires_build | link inputs require a native executable; remove --link, -l |
| 82 | next_value | man | E2000 | option_missing_value | {option} needs a value |
| 83 | parse_size | man | E2000 | option_invalid_value | {option} must be a byte count or a number followed by KiB, |
| 84 | run_formatter | man | E2000 | file_not_formatted | hand-built JSON line: file is not formatted |
| 85 | run_test_action | new | E2000 | index_out_of_range | test index is out of range; refresh the test list |
| 86 | run_test_action | man | W2001 | tool_notice | hand-built JSON warning line (messages of the build report |
| 87 | run_test_action | new | E2006 | tests_failed | {failed} test{} failed |
| 88 | debug_test | man | W2001 | tool_notice | hand-built JSON warning line (messages of the runner) |
| 89 | run_bench_action | new | E2000 | index_out_of_range | bench index is out of range; refresh the bench list |
| 90 | run_bench_action | man | W2001 | tool_notice | hand-built JSON warning line (messages of the bench report |
| 91 | run_bench_action | new | E2005 | benchmark_failed | benchmark '{}.{}' {} |
| 92 | new_project | new | E2000 | usage_error | usage: tsuzuri new directory [--namespace NAME] |
| 93 | fetch_dependencies | man | E2000 | usage_error | fetch takes one project directory and optionally --json |
| 94 | fetch_dependencies | man | E2000 | project_manifest_missing | fetch requires a Tsuzuri.toml in the project directory |
| 95 | publish_package | man | E2000 | usage_error | publish takes one package directory, --git URL, --rev COMM |
| 96 | publish_package | man | E2000 | invalid_publish_git_url | GIT_URL_RULE: the git url is not an allowed url |
| 97 | publish_package | man | E2000 | invalid_publish_git_rev | GIT_REV_RULE: the rev is not a full commit id |
| 98 | publish_package | man | E2000 | project_manifest_missing | publish requires a Tsuzuri.toml in the package directory |
| 99 | api_diff_usage | new | E2000 | usage_error | usage: tsuzuri api-diff old new [--json]; old and new are  |
| 100 | bindgen_command | new | E2000 | bindgen_argument_error | <<message>> |
| 101 | parse_repl_arguments | man | E2000 | option_requires_build | repl supports only the native target |
| 102 | parse_repl_arguments | man | E2000 | option_invalid_value | CPU tuning must be 'generic' or 'native' |
| 103 | parse_repl_arguments | man | E2000 | option_invalid_value | REPL_TIMEOUT: --timeout takes a number of seconds |
| 104 | parse_repl_arguments | man | E2000 | usage_error | REPL_USAGE |
| 105 | parse_repl_arguments | man | E2000 | duplicate_option | {option} specified more than once |
| 106 | repl_command | new | E2001 | repl_stream_failure | cannot read or write the REPL's standard streams: {error} |
| 107 | script_arguments | man | E2000 | option_missing_value | {option} needs a value |
| 108 | script_arguments | man | E2000 | usage_error | SCRIPT_OPTIONS |
| 109 | script_arguments | man | E2000 | usage_error | SCRIPT_FILE |
| 110 | main | man | W2001 | tool_notice | hand-built JSON warning line (messages of a script run) |

