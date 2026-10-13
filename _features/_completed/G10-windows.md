# G10: Windows ネイティブ対応

| 項目 | 内容 |
| --- | --- |
| ID | G10 |
| 優先度 | P3 |
| 規模 | M |
| 依存 | – |
| 後続 | G08（Windows debug 形式の拡張）、F01（ワーカープールの Windows 実装）。G14・G15・G16 が弱く依存した |
| 状態 | done |
| 起票 | P3 計画。2026-09-28 に Phase 1 を実装（実行ゲートだけ未確認で `blocked`）。2026-09-29 に残作業（Windows の実行ゲート）を実装者向けに詳細化（HEAD `f8dc655`） |
| 実装の基準 | `2282fd9`（PR #23 のマージ）、2026-10-13。ブランチ `features/g10-windows`、PR #24 |
| 承認 | 不要。D3 の「push と workflow の実行は人間が行う」は、利用者の依頼（コミット・push・プルリクエストの作成までを求め、判断を包括的に任せた）に従って実装者が行った。GUIDE の [D-48](../GUIDE.md#d-48-g10windows-ネイティブ対応の確定) に記録した |
| 改善する劣位 | C/C++ 比: 対応プラットフォームの幅（Windows の実行の確認） |
| 利用者向け仕様 | [対応プラットフォームとクロスコンパイル](../../_tsuzuri/language-reference/compiler/platforms.md)、[コンパイラの使い方](../../_tsuzuri/language-reference/compiler/usage.md#windows)、[言語仕様](../../docs/language.md)、[コンパイラ構成](../../docs/architecture.md) |

## 目的と実装範囲

Windows の GitHub Actions runner と開発者環境で、既存の native の実行ファイル・オブジェクト・ヘッダーの出力を安全に生成し、実行できるようにする。対象は x86_64 Windows（MSVC ABI）で、既存の言語機能と `Task.parallel` を含む native のテストを通す。新機能（ARM64 Windows の正式対応、MinGW、DLL、OS API）は足していない。

| 段階 | 内容 |
| --- | --- |
| Phase 1（2026-09-28） | MSVC ABI の native の flags（`native_compile_args`）、公開 wrapper だけの `dllexport`（`llvm::windows_abi`）、CRT への UTF-8 の出力、source と同じファイル（hard link を含む）への出力の拒否（`same-file`）と一時ファイルからの `fs::rename` による置換、Win32 の常駐 pool と Result の cancellation（`src/runtime/task-windows.h`）、言語内 test runner の Windows 分岐、専用 E2E（`tests/windows.mjs`・`tests/windows.rs`）。実行ゲートだけが未確認だった |
| 実行ゲート（2026-10-13） | 専用の workflow `.github/workflows/windows.yml` の復元。W1〜W8 を GitHub の `windows-2022` で実行し、見つかった不具合を分類して直した（「実行で見つかったもの」）。G15 の tier 1 の根拠になる検査（名前を付けた target、トラップ）を足し、文書と状態を更新した |

## 確定した仕様

### サポート範囲

| 項目 | 内容 |
| --- | --- |
| host | x64 Windows。CI は GitHub Actions の `windows-2022` |
| target | `x86_64-pc-windows-msvc`（`native_compile_args` が `--target=` で渡す。`--target x86_64-pc-windows-msvc` と名前を付けても同じ） |
| toolchain | LLVM 21.1.8 の `clang`（driver の形式。`clang-cl` の引数の形式は対象外）、Visual Studio 2022 の MSVC と CRT、Windows SDK |
| link | 実行ファイルは clang driver の既定のリンカー（MSVC の `link.exe`）。`tests/windows.mjs` の C host は `-fuse-ld=lld` |
| emit | `exe`（`.exe`）、`object`（`.obj`。ランタイムを含むものは `E2002`）、`llvm`、`header`。`--target wasm32` は host の OS に依存しない |
| `Task.parallel` | Win32 threads（`src/runtime/task.c` に `task-windows.h` を当てる）。WASM の backend は変わらない |
| CPU dispatch | portable baseline |
| ARM64 Windows | driver は `aarch64-pc-windows-msvc` を選ぶが、ゲートに含めない（D5） |

### 公開 ABI・console・出力保護の契約

- `exported` な関数の `tz_<name>` と、console の入口の `tsuzuri_main` だけが `dllexport` を持つ。内部関数 `@tz.fn.*` は internal のまま。C header は `__declspec(dllimport)` を付けない（object を直接リンクする利用者が対象）。
- stdout・stderr へは UTF-8 の bytes をそのまま書く。fd を binary mode にし、コードページは変えない。
- source と同じファイル（hard link を含む）への出力は `E2003`。symlink・directory・特殊ファイルへの出力も `E2003`。
- 既存の出力の置換は、一時ファイルからの `fs::rename` だけで行う。置換に失敗したら出力を消して再試行する fallback は使わない（D2）。
- Win32 の task runtime の失敗（`CreateThread`・`WaitForSingleObject`・`CloseHandle`・`InitOnceExecuteOnce` など）は、`Tsuzuri task runtime: <operation> failed (<code>)` を stderr に出して abort する。worker の上限 `TZ_TASK_MAX_THREADS = 32`、呼び出し元も work を実行すること、入れ子の逐次 fallback は、POSIX と共通の `src/runtime/task.c` による。
- 子プロセスへ渡すパスは、`fs::canonicalize` が返す `\\?\` 付きのものでなく、`cache::real_path` で外した形にする（共有の `\\?\UNC\server\share` は `\\server\share`）。Clang は verbatim パスの隣の引用 include を解決できず（`task_runtime_source_for` が `task-windows.h` を埋め込む理由）、Git for Windows は `GIT_CONFIG_GLOBAL` に verbatim パスを渡されると設定を読めない（`src/fetch.rs` の `Repository::create`）。
- native のトラップは、`llvm.trap` が `STATUS_ILLEGAL_INSTRUCTION`（0xC000001D）でプロセスを終わらせる。シグナルは使わない。プログラムが位置を標準エラーへ先に書き、`tsuzuri run` は `E2005`（`exit code: 0xc000001d`）、`tsuzuri test` と `bench` は `trapped or exited with code -1073741795` と報告する。

### 実行ゲート

次の W1〜W8 がすべて、D1 の環境の Windows x64 で成功したときにゲート合格とする。`windows.yml` はこの順でなく、W1・W2・W4〜W8・W3 の順に実行する（W3 が 40〜50 分で最も長いので最後にする。どの step も、前の step が失敗しても実行する）。

| ID | コマンド | 合格の条件 |
| --- | --- | --- |
| W1 | `cargo fmt --all -- --check` | 終了コード 0 |
| W2 | `cargo clippy --locked --all-targets -- -D warnings` | 終了コード 0 |
| W3 | `RUST_MIN_STACK=4194304 cargo test --locked --no-fail-fast` | 全 binary が `test result: ok`。`tests/windows.rs` は 4 件、`tests/lsp.rs` に `windows_file_uris_round_trip_drive_letters_and_canonical_paths ... ok` が出る |
| W4 | `cargo build --release --locked` | `target\release\tsuzuri.exe` ができる |
| W5 | `node tests/windows.mjs target/release/tsuzuri.exe` | 最後の行が `Windows MSVC: native O0/O3, Win32 tasks/results/failure injection, UTF-8, hardlinks, atomic replacement, object ABI and WASM passed`。その前に、名前を付けた target とトラップの 2 行が出る |
| W6 | `node tests/cache.mjs target/release/tsuzuri.exe` | 最後の行が `Cache: SHA-256, byte-identical hits, ...sidecars passed` |
| W7 | `node tests/examples.mjs target/release/tsuzuri.exe` | 最後の行が `Examples: console, higher-order functions, native C host, desktop ABI, HTTP/WASM loading, 32,600 game steps`（Windows では desktop の Python の部分を harness が飛ばす） |
| W8 | `node tests/wasm_threads.mjs target/release/tsuzuri.exe` | 終了コード 0。`WASM threads O0:`・`WASM tasks O0:`・`WASM threads O3:`・`WASM tasks O3:` で始まる行が出る |

参考の記録（W1〜W8 に含めない。D4）: `tests/e2e.mjs`、`tests/numeric_casts.mjs`。`numeric_casts.mjs` は workflow の step として走らせる（下の「旧計画から外れた判断」）。`e2e.mjs` は走らせない。

### 診断

新しい診断はない。実行ゲートで出ることを確かめる既存の診断は次のとおり（位置はすべて `Span::default()`）。

| コード | 条件 | メッセージ |
| --- | --- | --- |
| `E2002` | Windows でタスク・CPU・IO のランタイムを含む `--emit object` | `Windows COFF objects with embedded task, CPU, or IO runtime are not supported; emit LLVM and link the runtime once, or build an executable` |
| `E2002` | `TSUZURI_CLANG` の実行ファイルがない | `install LLVM/Clang 17+ or set TSUZURI_CLANG to its executable` を含む |
| `E2003` | 出力が source と同じファイル（hard link を含む） | `refusing to overwrite the input source` |
| `E2003` | 出力が symlink・directory・特殊ファイル | `the output must not be a symlink, directory, or special file` |
| `E2001` | Windows のファイルの同一性を調べられない | `cannot compare file identity '<path>': <error>` |
| `E2000` | `llvm::emit_with_export_style` に wasm と `ExportStyle::Dllexport` を渡した（API だけ） | `dllexport requires a native target` |
| `E2005` | native のトラップ | `program terminated with exit code: 0xc000001d; ...`（位置は、プログラムが書いた `trap: <理由> at <path>:<line>:<column>`） |

## 旧計画から外れた判断

HEAD `f8dc655` で書いた計画を、着手時の `2282fd9` と照らして見直した。

| 旧計画の記述 | 確定した判断と理由 |
| --- | --- |
| 更新する文書は `_docs/tools/build-and-cache.md`、`_docs/get-started.md`、`_docs/README.md`、`_docs/feature-status.md`。確認は `check-docs.mjs _docs README.md` | `_docs/` は `_tsuzuri/language-reference/` へ移り（GUIDE §8.1）、`feature-status.md` はなくなった。「未確認」と書いていたのは、`compiler/platforms.md`・`usage.md`・`native-interop.md`・`languages/strategy.md`・`why-tsuzuri.md`、`docs/architecture.md`・`docs/language.md`、`README.md`、`_features/README.md`。計画が挙げた `README.md`・`docs/architecture.md` の「Windows runnerでの実行ゲートは未確認です」の文は、すでに別の書き方に変わっていた |
| HEAD の `.github/workflows/` は `vscode.yml` だけ | `targets.yml`（G15）も足されていて、「Windows hosts are not here: tests/windows.rs, tests/windows.mjs and vscode.yml cover them」と書いてある。`vscode.yml` の Windows の job は、すでに `windows-2022` と LLVM 21.1.8 で MSVC の `Async.block_on`・`Net`・PDB を実行していて、runner の環境の実績だった。`windows.yml` は `targets.yml` が除いた Windows の実行を受け持つ |
| workflow の YAML: `actions/checkout@v4`・`setup-node@v4`、trigger は `push`・`pull_request`・`workflow_dispatch`、`timeout-minutes: 90`、step は上から順に、`cargo test --locked` | 現行の workflow に合わせた。`actions/checkout@v7`・`setup-node@v7`。trigger は `workflow_dispatch`、`pull_request`（`src/**`・`std/**`・`tests/**`・`examples/**`・`Cargo.*`・workflow 自身）、`push`（`main`）で、branch への `push` は二重に走るので外した。concurrency は PR だけ cancel（`targets.yml` と同じ）。`timeout-minutes: 120`（W3 だけで 40〜50 分、全体で約 50 分）。`ilammy/msvc-dev-cmd` は、actions/ と dtolnay/ の外で唯一の action なので commit に固定した（v1.13.0）。`tests/numeric_casts.mjs` は 13 秒で成功するので、step として残した（参考の記録から昇格）。`e2e.mjs` は入れない。step は `!cancelled()` で前の失敗に左右されず、`cargo test` が最後（`targets.yml` と同じ）。LLVM の取得は 3 回まで再試行する。`Show tools` が LLVM 21.1.8・MSVC の `link.exe`・LF の checkout を検査する。`cargo test` は `RUST_MIN_STACK=4194304` と `--no-fail-fast`（GUIDE D-45） |
| W3: `tests/windows.rs` は `3 passed` | 4 件。E09 が Net の cross-link のテストを足した |
| D4: 参考の suite（`e2e.mjs`・`numeric_casts.mjs`）を手順 7 で実行して記録する | 診断のために run で 1 回ずつ実行して結果を記録した（下）。`e2e.mjs` は、ゲートにも最終の workflow にも入れない。`numeric_casts.mjs` は 13 秒で成功したので、W1〜W8 には数えず、workflow の step として残した |
| 対象外: PDB／CodeView | G16 Phase 3 が実装済みで、`vscode.yml` の Windows x64 の job が MSVC の PDB と natvis を検査している。G10 のゲートは PDB を検査しない |
| G15 は「G10 の job」で Windows の tier を上げる | `tests/windows.mjs` に、名前を付けた `x86_64-pc-windows-msvc` を `-O0`・`-O3` で `build`・`run`・`test` する検査（`tests/targets.mjs` と同じ内容）を足し、`platforms.md` の tier を 2 から 1 にした。`aarch64-pc-windows-msvc` と MinGW は Windows のホストで実行していないので 2 のまま |
| トラップは W5 の対象でない | 「数値・トラップ・native と WASM の差」を言い切れるよう、W5 に、`llvm.trap` が 0xC000001D で終わり、`E2005` と位置が出ることを足した |
| D6: 直してよいのは、期待値を変えない harness、`cfg(windows)` の分岐、`task-windows.h`、`windows_abi` | `src/fetch.rs` の 1 か所を `cfg!(windows)` の分岐で直し、`cache::real_path` が共有の verbatim パス（`\\?\UNC\...`）も外すようにした（Copilot の指摘。`\\?\` で始まる path にだけ効くので、Unix では何もしない）。Rust のテスト 7 件と `packages.rs` の 4 か所は、Unix の期待値を変えずに、Windows の側の定義を足した（`-lm` を渡さない、実行ファイルの拡張子、トラップの終了の文言、パス区切り、CRLF、128-bit の helper）。トラップの文言は、`0xC000001D` を符号付きの終了コードにした `-1073741795` を期待値にした。実行結果からは写していない |
| E10・E08・E13・E14・E11 は Windows の作業を「G10 の後」としていた | G10 は実行の確認までで、OS API・DLL・トラップの報告の改善・LLP64 の bindgen・パスの予約名は含まない。後続の作業として、下に一覧した |

## 決定事項（確定）

| ID | 決定 |
| --- | --- |
| D1 | 環境: x64 Windows、LLVM 21.1.8（`LLVM-21.1.8-win64.exe`、SHA-256 `7a5386c26497db1691f320121e5b113364dd0274b98e55f15f4dbc00c0450113`）、Visual Studio 2022 の VCTools と Windows SDK、Rust stable の MSVC toolchain、Node.js 24。`vscode.yml` と同じ版で、runner の image の LLVM の版の揺れを除く。Node 20 は重い BigInt の suite で V8 が異常終了した記録がある |
| D2 | 既存の出力の置換は `publish_outputs` の `fs::rename` だけ。runner で置換と旧出力の保持は成功した。古い出力を消してから rename する fallback は入れていない |
| D3 | `.github/workflows/windows.yml` を復元する。workflow 名 `Windows MSVC`、job id `windows`、`windows-2022`（G15 がこの名前を前提にする）。trigger は上の表のとおり現行の convention に合わせた。push と PR は、利用者の依頼に従って実装者が行った |
| D4 | ゲートは W1〜W8。`tests/e2e.mjs`・`tests/numeric_casts.mjs` は参考（`numeric_casts.mjs` は参考のまま、workflow の step として走らせる）。POSIX 専用の harness（`tests/tasks.mjs` など。`-pthread` を無条件に渡す）は実行しない |
| D5 | ARM64 Windows はゲートに含めない。`tests/windows.mjs` は `--target=x86_64-pc-windows-msvc` を固定で渡す |
| D6 | 失敗時に直してよい範囲。実際に使った範囲は、`task-windows.h`、`src/fetch.rs` の `cfg!(windows)` の分岐、`\\?\` で始まる path にだけ効く `cache::real_path`、harness と Rust のテスト |
| D7 | CI と clone で `core.autocrlf=false`。`.gitattributes` は足さない（全 host の checkout に効く repository 全体の変更）。`Show tools` が `src/runtime/string.ll` の LF を検査する |
| D8 | ランタイムを含む COFF の object は `E2002` のまま。exe と、LLVM IR とランタイムを一度だけリンクする経路を提供する。複数 object の weak 統合は対象外 |
| D9 | hard link の同一性は、Windows 限定の `same-file` 1.0.6 の `is_same_file`（GUIDE D-29）。runner で `E2003` が出ることを確かめた |
| D10 | `done` は、x64 で W1〜W8 がすべて成功し、復元した workflow に GitHub の緑の run があるときだけ。満たした（下の記録） |
| D11 | tier 1 の根拠は、windows.yml の job が、名前を付けた `x86_64-pc-windows-msvc` を `-O0`・`-O3` で `build`・`run`・`test` すること（G15 の tier の定義と、`tests/windows.mjs` の検査）。aarch64 の MSVC と MinGW は 2 のまま |
| D12 | Windows の OS API・DLL・トラップの報告の改善などは、このチケットに含めない。診断のメッセージにある `(G10)` は、その作業を指す印として残し、メッセージと既存のテストは変えない |

## 受け入れ条件

- [x] x64 Windows の runner（GitHub の `windows-2022`）で W1〜W8 がすべて成功し、下の形式で記録した。
- [x] `.github/workflows/windows.yml` を復元し、GitHub で緑の run がある（URL を下に記録した）。
- [x] `Task.parallel` が Windows native で動き、failure injection の 3 つの mode が明示診断で abort する（W5）。
- [x] Windows で source の hard link への出力が `E2003` になり、既存の出力の置換が成功する（W3・W5）。
- [x] 公開 wrapper が `dllexport` を持つ（W3・W5）。
- [x] コードは `src/runtime/task-windows.h`、`src/fetch.rs` の `cfg!(windows)` の分岐、`\\?\` で始まる path にだけ効く `cache::real_path` だけを変えた。Unix・WASM の挙動と生成 IR は変わらない。
- [x] 文書と状態を更新し、`node scripts/check-docs.mjs` が成功する。

## 実装と検証（2026-10-13）

### 実装したファイル

- `.github/workflows/windows.yml`（復元）。
- `src/runtime/task-windows.h`（`WIN32_LEAN_AND_MEAN`・`_CRT_SECURE_NO_WARNINGS` を、未定義のときだけ定義）、`src/fetch.rs`（`Repository::create` が `cfg!(windows)` で `cache::real_path` を使う）、`src/cache.rs`（`real_path` が `\\?\UNC\server\share` を `\\server\share` にする。`plain_path` の単体テストと、管理共有 `\\localhost\C$` の Windows 限定のテスト）。
- テスト: `tests/windows.mjs`（stderr の最後の JSON 行を読む、名前を付けた target、トラップ）、`src/test_runner.rs`（test module の CRLF）、`tests/bench.rs`・`fixed_arrays.rs`・`host_abi.rs`・`packages.rs`・`property.rs`・`test_runner.rs`・`trap_locations.rs`。
- 文書: `README.md`、`docs/architecture.md`、`docs/language.md`、`_tsuzuri/language-reference/` の `compiler/platforms.md`・`usage.md`・`native-interop.md`、`languages/strategy.md`・`why-tsuzuri.md`、`built-in-types-and-modules/test.md`、`types-and-type-inference/basic-types.md`、`_features/README.md`、GUIDE の D-48、`AGENTS.md`。

### Windows 実行記録（2026-10-13）

- 環境: GitHub の `windows-2022`（Windows Server 2022、`10.0.20348`）。AMD EPYC 9V74 80-Core Processor、論理 4 コア。
- 版: rustc 1.99.0（b940084d7 2026-09-28）、clang 21.1.8、LLD 21.1.8、node v24.21.0、MSVC 14.44.35207（`...\VC\Tools\MSVC\14.44.35207\bin\HostX64\x64\link.exe`）、Windows SDK 10.0.26100.0、Visual Studio 2022 Enterprise。commit `57334e0`（push の run が検査した commit。その後、先頭の commit の CRLF の修正を畳み、main の A17・A18 のチケットの commit へ rebase したので、同じ変更が付け替わった）。この run の後の変更は、workflow の整形（trigger・timeout・action の固定・`numeric_casts.mjs` の step）、`src/fetch.rs` の `cfg!(windows)` の分岐、`src/cache.rs` の `real_path`（共有のパス。Copilot の指摘）、文書で、その検査は PR #24 の run（`pull_request`）が受け持つ。run の URL は PR の checks と PR の説明にある（記録の commit が検査される run の URL は、その commit には書けない）。
- run: https://github.com/tatsuya-midorikawa/Tsuzuri/actions/runs/38341607512（全体で 50 分）
- W1〜W8:

| ID | 結果 | 所要時間 |
| --- | --- | --- |
| W1 | 成功 | 5 秒 |
| W2 | 成功 | 36 秒 |
| W4 | 成功 | 3 分 47 秒 |
| W5 | 成功 | 35 秒 |
| W6 | 成功 | 12 秒 |
| W7 | 成功 | 14 秒 |
| W8 | 成功 | 49 秒 |
| W3 | 成功（96 の `test result` で 1,204 件が成功、失敗 0、`tests/windows.rs` は 4 件） | 41 分 59 秒 |

- 参考: `tests/numeric_casts.mjs` は成功（1585 件、13 秒）。`tests/e2e.mjs` は、最初の native のトラップの検査（`tests/e2e.mjs:759`）で止まる。host の C の `signal(SIGILL, ...)` の handler が呼ばれず、終了コードが期待の 86 でなく 0xC000001D になる。それより前の native と WASM の結果の照合は一致した。`llvm.trap` の終わり方は上の契約のとおりで、harness の期待が POSIX の前提である。`e2e.mjs` は D4 のとおりゲートにも workflow にも入れていない。`numeric_casts.mjs` は workflow の step として走らせる（W1〜W8 には数えない）。
- 修正: 下の表。

### 実行で見つかったもの

最初の run（W5 は 8 秒で止まり、W3 は 8 つの target で 17 件失敗）から、4 回目の run（4 回の push）で W1〜W8 がすべて緑になった。途中の確認には、速い step だけの throwaway branch の run を 3 回使った。分類は、元の計画の手順 8 の表による。

| # | 見つけた場所 | 症状 | 分類 | 修正 |
| --- | --- | --- | --- | --- |
| 1 | W5 の `tests/task_runtime.c` の compile | `task-windows.h:4:9: error: '_CRT_SECURE_NO_WARNINGS' macro redefined [-Werror,-Wmacro-redefined]`。harness は `-D_CRT_SECURE_NO_WARNINGS` と `-Werror` を渡す | Windows 分岐（`task-windows.h`） | 未定義のときだけ定義する（`async.c`・`net.c` と同じ）。harness は、この組を保つ |
| 2 | W5 の `E2002` の検査 | stderr の先頭に `W1001`（`tests/fixtures/tasks` の未使用の局所変数。G03 以後の警告）の JSON 行が出て、`JSON.parse` が失敗 | harness | stderr の最後の JSON 行を読む（`tests/targets.mjs` と同じ） |
| 3 | W3 `--lib` の `bench_runner_rejects_malformed_arguments` | C の text mode の stdout が CRLF | harness（Rust のテスト） | 比べる前に改行を正規化する。`parse_bench_output` は `lines()` で読むので、製品は影響を受けない |
| 4 | W3 `packages` の 10 件 | git／registry の取得がすべて `E2007`（`fatal: unknown error occurred while reading the configuration files`）。手元の Windows で再現し、`GIT_CONFIG_GLOBAL=\\?\C:\...` が原因と確かめた | Windows 分岐（製品の不具合。E10 は「Windows での git は未検証（G10）」と記していた） | 一時ディレクトリは canonicalize で verbatim になるので、`Repository::create` が `cfg!(windows)` で `cache::real_path` を使う |
| 5 | W3 `packages` の 4 か所の `contains` | 診断のパスが `\` 区切り | harness | プラットフォームの区切りで比べる |
| 6 | W3 `fixed_arrays`・`host_abi`・`trap_locations` | `clang: error: linker command failed with exit code 1181`。`-lm` が `m.lib` を探す | harness | Windows では `-lm` を渡さない。実行ファイルに `EXE_SUFFIX` を付ける |
| 7 | W3 `bench`・`property`・`test_runner` | `trapped or terminated by signal` を期待していたが、Windows は `trapped or exited with code -1073741795` | harness（Windows の側の定義を足した） | Unix の期待値は変えない。`-1073741795` は 0xC000001D の符号付きの値 |
| 8 | W3 `trap_locations` の native | `LNK1120`。MSVC には compiler-rt がなく、`d128` が使う `__udivti3`・`__umodti3` が未解決。リンカーの診断は stdout に出るので、テストの panic には出ない | harness | Windows ではテストが `wasm.ll` の helper を足す（`windows_abi` と同じ。手元で `clang -c` と `llvm-nm` で未解決が消えることを確かめた） |

### 確かめたこと

- 上の run で、W1〜W8 が緑になった。W5 は、O0・O3 の UTF-8 の出力、既存の実行ファイルの再ビルドと置換、`TSUZURI_CLANG` がないときの `E2002` と旧出力の保持、`examples/tasks`（`21325334000`）・`examples/control`（`42`）、`tests/task_runtime.c` の topology 4 つ、`tests/windows_runtime.c` の `create`・`wait`・`close` の注入、`dllexport` の IR と C host、runtime 入りの object の `E2002`、source への hard link の `E2003`、`tz_answer` の object と C host、wasm32 の import なし、名前を付けた `x86_64-pc-windows-msvc` の build・run・test、トラップを確かめた。
- 変更が Unix・WASM に影響しないこと: `task-windows.h` は Windows の target でだけ埋め込まれ、`src/fetch.rs` は `cfg!(windows)` の分岐の中だけを、`real_path` は `\\?\` で始まる path だけを変えた。`cargo fmt --all -- --check` は成功。
- 手元（Windows 11 x64、Rust の GNU toolchain、同梱の MinGW）では、`tests/packages.rs` の 21 件、`tests/trap_locations.rs` の 7 件、`--lib` の 193 件と、新しい harness の検査（`--target native`）を確かめた。MSVC の link は手元になく、runner の結果だけを根拠にする。

### 確かめていないこと

- ARM64 Windows、MinGW のホストでの名前付き target の実行、`--emit shared`、OS API。
- MSVC の実行ファイルを Windows 以外のホストでリンクすること。
- `tests/e2e.mjs` と POSIX 専用の harness（`tests/tasks.mjs`・`tests/features.mjs` など）の Windows での実行。

### 後続の作業（G10 の後に残る Windows の作業）

- Windows の OS API（E08 の Phase 2）: `File`・`Dir`・`Env`・`Time`・`Random`・`Process` の Win32 の実装。いまは `E2002`（`OS_WINDOWS_MESSAGE`）。
- Windows の共有ライブラリと DLL の方針（E13）。いまは `--emit shared` が `E2000`。
- トラップの報告とスタック枯渇の分類（E14）: vectored exception handler による同じ報告。`--trap-mode return` の COFF object は `E2002`。`tests/stack_overflow.mjs` は Windows で飛ばす。
- bindgen の LLP64（E11）、パッケージのパスの予約名の検査（E10）。
- ランタイムを含む COFF の `--emit object`（D8）。
- `tests/e2e.mjs` など POSIX の前提を持つ harness の Windows 対応（トラップの終了コードの期待）。
- ARM64 Windows の実行と、MinGW のホストの名前付き target の実行（G15 の tier）。
- `Cargo.toml` の `rust-version`（1.85）と、`if ... && let`（let chain。1.88 から安定）の食い違い。1.85 では確かめていない。