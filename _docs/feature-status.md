# 対応状況と機能別索引

[ドキュメントのトップ](README.md)

対象は Tsuzuri 0.1.0、2026-09-29 時点のリポジトリです。機能チケット全93件（第1期の55件と、2026-09-29 に起票した第2期の計画38件）について、現在の提供範囲と利用者向けの説明を対応付けています。チケットの done は、そのチケットで合意した段階の完了であり、当初の設計案の全項目や他言語との互換性を意味しません。「未着手（計画）」の行は改善計画であり、現在の提供機能ではありません。

## 基礎言語

チケット化以前の機能も言語リファレンスに含めます。[字句とインデント](language-reference/lexical-and-layout.md)、[値と定数](language-reference/values-and-constants.md)、[関数と再帰](language-reference/functions.md)、[型と推論](language-reference/types.md)、[数値](language-reference/numbers.md)、[評価順序](language-reference/expressions-and-operators.md)、[所有権](language-reference/ownership.md)、[制御構文](language-reference/control-flow.md)が入口です。

## A 型システム

| ID | 現在の提供範囲 | 解説 | 実装記録 |
| --- | --- | --- | --- |
| A01 | 型適用、ジェネリックレコード、具体化ごとの所有権 | [レコード](language-reference/records.md) | [A01](../_features/_completed/A01-generic-records.md) |
| A02 | union、payload、列挙型、case 関数値 | [union](language-reference/unions.md) | [A02](../_features/_completed/A02-union-types.md) |
| A03 | match の網羅性エラーと到達不能節の警告 | [パターン](language-reference/patterns.md) | [A03](../_features/_completed/A03-match-exhaustiveness.md) |
| A04 | 有限値を持つ再帰型、所有ノード、反復 clone / drop | [再帰型](language-reference/unions.md) | [A04](../_features/_completed/A04-recursive-types.md) |
| A05 | 透過的な型別名、汎用別名、循環検査 | [型別名](language-reference/records.md) | [A05](../_features/_completed/A05-type-aliases.md) |
| A06 | 条件付き instance、superclass、default method | [型クラス](language-reference/generics-and-typeclasses.md) | [A06](../_features/_completed/A06-typeclass-extensions.md) |
| A07 | Eq / Ord / Display / Hash / Default の導出 | [deriving](language-reference/deriving.md) | [A07](../_features/_completed/A07-deriving.md) |
| A08 | UTF-16 char と Unicode scalar の utf8char | [文字](language-reference/strings-and-characters.md) | [A08](../_features/_completed/A08-char-type.md) |
| A09 | 共有借用フィールドと単一 region の名前付き契約 | [lifetime](language-reference/lifetimes.md) | [A09](../_features/_completed/A09-named-lifetimes.md) |
| A10 | 明示 kind、rank-1 HKT、末尾固定の部分適用 | [高階型](language-reference/higher-kinds.md) | [A10](../_features/_completed/A10-higher-kinded-types.md) |
| A11 | 比較の非消費化、借用 Eq / Ord、構造比較 | [比較クラス](language-reference/generics-and-typeclasses.md) | [A11](../_features/_completed/A11-borrowed-comparisons.md) |
| A12 | 対応（Phase 1・2）: レコードごとに 16 個までの独立した region（`record Pair {r s}`）。所有権検査は不変の束縛・field の読み出し・直接の完全適用で region ごとに借用を分け、可変の束縛・ループ・関数値・コレクションでは全 region を保持する。Phase 2 で名前付き関数の引数に region で量化した関数型（`({s} ref {s} T -> ref {s} T)`）。渡す関数の契約を検査し、その関数は直接の完全適用だけ。region は生成 IR を変えない。戻り値・ローカル・field の量化型と region 間の outlives 制約は未実装 | [lifetime](language-reference/lifetimes.md) | [A12](../_features/_completed/A12-multiple-regions.md) |
| A13 | 未着手（計画）: 排他借用フィールド | [lifetime](language-reference/lifetimes.md) | [A13](../_features/A13-exclusive-borrow-fields.md) |
| A14 | 未着手（計画）: 型クラスによる動的ディスパッチ | [型クラス](language-reference/generics-and-typeclasses.md) | [A14](../_features/A14-dynamic-dispatch.md) |
| A15 | 対応（Phase 1・2）: `--warn implicit-copy` で配列・リストの暗黙の複製を `W1006` として報告（既定は無効、生成コードは不変）。明示の `Array.copy` / `List.copy`。複製の一覧 `copies::sites` は debug build で生成した複製と照合する。LSP は同じ位置に複製の種類の inlay hint を返す。複製の省略（PM07）と共有バッファによる O(1) 化（C10）は未実装 | [所有権](language-reference/ownership.md) | [A15](../_features/_completed/A15-copy-cost-visibility.md) |
| A16 | 未着手（計画）: 固定長配列と const ジェネリクス | [型](language-reference/types.md) | [A16](../_features/A16-fixed-arrays.md) |

## B エラー処理と制御

| ID | 現在の提供範囲 | 解説 | 実装記録 |
| --- | --- | --- | --- |
| B01 | Option / Result の型と所有・借用 API | [Option / Result](library-reference/option-result.md) | [B01](../_features/_completed/B01-option-result.md) |
| B02 | Option / Result ビルダーで失敗時に継続を短絡 | [計算式](language-reference/computation-expressions.md) | [B02](../_features/_completed/B02-result-propagation.md) |
| B03 | 通常ループの break / continue と解放 | [ループ](language-reference/control-flow.md) | [B03](../_features/_completed/B03-break-continue.md) |
| B04 | bool / Option の部分認識器、明示 union の複数 case | [アクティブパターン](language-reference/active-patterns.md) | [B04](../_features/_completed/B04-active-pattern-extensions.md) |
| B05 | match! / and!、BindReturn / Bind2。use / use! は B07 で対応、try は未対応 | [計算式の合成](language-reference/computation-expressions.md) | [B05](../_features/_completed/B05-computation-expression-extensions.md) |
| B06 | parallel_results の未開始停止と最小 index の Error | [Task](language-reference/tasks.md) | [B06](../_features/_completed/B06-task-cancellation.md) |
| B07 | 対応: 利用者が宣言した record・union の `instance Drop<T>`。scope の終わり・置き換え・コレクションの要素・未実行の Task の捕捉値・再帰 union のノードで一度だけ `drop` を呼び、field を宣言順に解放する（native と WASM、100 万段の再帰 union を検証）。Drop 型は非 Copy で、field の move と更新は `E1012`。Phase 2 で `use` / `use!` 束縛、早期解放の `Owned.drop`、Drop 型を捕捉できる非 Copy の関数値 `Owned.function` / `Owned.call`。`extern type` への直接の `Drop` は未実装 | [所有権](language-reference/ownership.md) | [B07](../_features/_completed/B07-user-drop.md) |
| B08 | 未着手（計画）: 非同期計算とホスト駆動の実行 | [Task](language-reference/tasks.md) | [B08](../_features/B08-async.md) |

## C コレクションとデータ

| ID | 現在の提供範囲 | 解説 | 実装記録 |
| --- | --- | --- | --- |
| C01 | Array / List の消費的更新、一意領域の再利用 | [所有更新](library-reference/arrays-and-lists.md) | [C01](../_features/_completed/C01-consuming-update.md) |
| C02 | 非 Copy の Vec、容量管理、Array との移送 | [Vec](library-reference/vec.md) | [C02](../_features/_completed/C02-growable-vec.md) |
| C03 | 配列の半開共有スライスと寿命検査 | [スライス](library-reference/arrays-and-lists.md) | [C03](../_features/_completed/C03-slices.md) |
| C04 | 借用 bulk API、安定 sort、検索、fold、集計 | [Array / List](library-reference/arrays-and-lists.md) | [C04](../_features/_completed/C04-bulk-array-api.md) |
| C05 | 所有レコードのフィールド更新 | [レコード更新](language-reference/records.md) | [C05](../_features/_completed/C05-record-update-syntax.md) |
| C06 | 不透明な順序付き Map / Set、非 Copy キーの借用 | [Map / Set](library-reference/map-set.md) | [C06](../_features/_completed/C06-map-set.md) |
| C07 | 一回消費 Seq、明示 iter、借用要素の反復 | [Seq](library-reference/sequences.md) | [C07](../_features/_completed/C07-iteration-protocol.md) |
| C08 | 未着手（計画）: 可変スライスと要素のその場更新 | [Array / List](library-reference/arrays-and-lists.md) | [C08](../_features/C08-mutable-slices.md) |
| C09 | 対応（Phase 1・2）: 不透明で非 Copy の `HashMap` / `HashSet`。キーは `Hash` と `Eq`、検索・挿入・削除は平均 O(1)。反復順は挿入順で、削除は末尾の entry を移す swap-remove のため、ハッシュ値・target・seed に依らず native と WASM で一致する。`with_seed` と `randomized`（OS の乱数。既定の wasm32 では `E2000`）による SipHash-1-3 の seed 付きハッシュ、借用キーの `contains_key_ref` / `get_ref` / `at_ref` / `remove_ref`、診断の `longest_probe`。既定の map は HashDoS への耐性がなく、seed 付きでも 64-bit の digest が完全に衝突するキーは防げない。SIMD のグループ探索（F08 待ち）、縮小、集合演算、`singleton`、`pop` は未実装 | [HashMap / HashSet](library-reference/hash-map.md) | [C09](../_features/_completed/C09-hash-map.md) |
| C10 | 未着手（計画）: Arena / Handle による循環構造、参照カウントの検討 | [所有権](language-reference/ownership.md) | [C10](../_features/C10-shared-ownership.md) |
| C11 | 未着手（計画）: 多次元配列と行列カーネル | [Array / List](library-reference/arrays-and-lists.md) | [C11](../_features/C11-multidimensional-arrays.md) |

## D 文字列と数値

| ID | 現在の提供範囲 | 解説 | 実装記録 |
| --- | --- | --- | --- |
| D01 | Display / Parse、最短往復表示、所有文字列化 | [表示と解析](library-reference/formatting-and-parsing.md) | [D01](../_features/_completed/D01-display-parse-format.md) |
| D02 | 検索、分割、連結、切り出し、ASCII 操作、符号化・移送 | [文字列 API](library-reference/text.md) | [D02](../_features/_completed/D02-string-library.md) |
| D03 | 全 Float の基本演算、f32 / f64 の超越関数 | [Math](library-reference/math.md) | [D03](../_features/_completed/D03-generic-math.md) |
| D04 | checked / saturating、bit、rotate、拡大乗算 | [Int](library-reference/integers.md) | [D04](../_features/_completed/D04-integer-intrinsics.md) |
| D05 | 明示 FMA、固定木、補償和、順序付き内積 | [数値集計](library-reference/math.md) | [D05](../_features/_completed/D05-fma-ordered-reductions.md) |
| D06 | 具体型 const、前方参照、限られた式の評価 | [定数](language-reference/values-and-constants.md) | [D06](../_features/_completed/D06-compile-time-constants.md) |
| D07 | 対応（Phase 1 の段 A・B と Phase 2 の `Format` クラス）: `$"..."` と `u8$"..."` の文字列補間（`{expr}`、`{expr:spec}`、`{{` と `}}`）。穴は借用して左から右へ一度ずつ評価し、結果は一度だけ確保する。書式指定 `[[fill]align][+][width][.precision][type]`（type は `x X o b e f`）の数値は実行時が厳密に整形し、native と WASM で一致する。record・union の穴は `Format` クラスの instance へ検証済みの spec を渡して整形でき、`Format.parse` と `Format.pad` を使える。書記素幅（D09 待ち）、`#` と `0` フラグ、locale と桁区切り、実行時に組み立てる書式文字列、`deriving (Format)` は未実装 | [表示と解析](library-reference/formatting-and-parsing.md) | [D07](../_features/_completed/D07-string-interpolation.md) |
| D08 | 未着手（計画）: JSON と Encode / Decode の導出 | [deriving](language-reference/deriving.md) | [D08](../_features/D08-json-serialization.md) |
| D09 | 未着手（計画）: 線形時間の正規表現、Unicode 正規化と書記素 | [文字列 API](library-reference/text.md) | [D09](../_features/D09-regex-unicode.md) |
| D10 | 未着手（計画）: 意味を保った f16 のハードウェア演算 | [数値](language-reference/numbers.md) | [D10](../_features/D10-f16-hardware.md) |
| D11 | 未着手（計画）: const 関数と表のコンパイル時生成 | [定数](language-reference/values-and-constants.md) | [D11](../_features/D11-const-evaluation.md) |

## E モジュールとホスト

| ID | 現在の提供範囲 | 解説 | 実装記録 |
| --- | --- | --- | --- |
| E01 | private、公開型からの漏れ検査 | [可視性](language-reference/modules-and-packages.md) | [E01](../_features/_completed/E01-visibility.md) |
| E02 | std 埋め込み、予約名、未使用コードの除去 | [標準モジュール](language-reference/modules-and-packages.md) | [E02](../_features/_completed/E02-standard-library-infrastructure.md) |
| E03 | ディレクトリ階層と再帰的ソース探索 | [階層モジュール](language-reference/modules-and-packages.md) | [E03](../_features/_completed/E03-hierarchical-modules.md) |
| E04 | 限定 manifest とローカル path 依存 | [パッケージ](language-reference/modules-and-packages.md) | [E04](../_features/_completed/E04-packages.md) |
| E05 | 借用入力、所有バッファ結果、スカラーレコード | [C ABI](guides/native-interop.md)、[WASM](guides/webassembly.md) | [E05](../_features/_completed/E05-host-abi-buffers.md) |
| E06 | 同期 extern、到達する import のみ生成 | [外部関数](guides/native-interop.md) | [E06](../_features/_completed/E06-host-imports.md) |
| E07 | Debug.print / trace、native stderr と WASM opt-in | [Debug](tools/debugging.md) | [E07](../_features/_completed/E07-debug-output.md) |
| E08 | 対応（Phase 1 の段 A〜C と Phase 2 の `Process`・metadata・`Dir.walk`。Windows を除く）: `File`・`Dir`・`Path`・`Env`・`Time`・`Random`・`Os`・`Process` の std モジュール。OS に触れる操作は `IO<Result<_, Os.Error>>` の遅延アクションで、`Path` と `Random.Pcg` は純粋。`IO<i32>` の入口の値が終了コードになる。既定の wasm32 は OS API を `E2000` で拒否し、`--wasm-host wasi`（WASI preview1）では native と同じ結果を返す（`Process.run` は `Other`）。Windows の native は `E2002`（G10 待ち）。WASI preview2 とコンポーネントモデルは未実装 | [OS API](library-reference/os.md) | [E08](../_features/_completed/E08-os-api.md) |
| E09 | 未着手（計画）: TCP / UDP | [IO](library-reference/io.md) | [E09](../_features/E09-network.md) |
| E10 | 未着手（計画）: git 依存、lockfile、版解決と registry | [パッケージ](language-reference/modules-and-packages.md) | [E10](../_features/E10-package-registry.md) |
| E11 | 未着手（計画）: C ヘッダーからの extern 生成 | [C ABI](guides/native-interop.md) | [E11](../_features/E11-c-bindgen.md) |
| E12 | 対応（Phase 1 と一部の拡張）: `extern "symbol" def` と `extern "module" "symbol" def` のリンク名、`--link`・`-l`・`-L` と manifest の `[native]` でのホストのリンク指定（native の実行ファイル。root が `native = true` で許可した依存 package の `[native]` を含む）、`extern type` の不透明ハンドル、捕捉のないトップレベル関数の静的コールバック（ホストの別スレッドからの呼び出しと複数スレッドからの同時呼び出しを検証済み）。捕捉のある関数値のコールバック、`Option<H>` と NULL の対応、i128・f16・タプルの ABI、`extern type` への `Drop` は未実装 | [外部関数](guides/native-interop.md) | [E12](../_features/_completed/E12-ffi-extensions.md) |
| E13 | 未着手（計画）: TypeScript などのバインディングと Web glue の生成 | [WASM](guides/webassembly.md) | [E13](../_features/E13-host-bindings.md) |
| E14 | 対応（Phase 1・2・3）: WASM の `createBoundary` がトラップとスタック枯渇を値で返し、失敗した instance を捨てて作り直す。native object は `--trap-mode return` で `tsuzuri_try_<name>`（status 0・1・2 と `tsuzuri_trap_info`）を出し、トラップした呼び出しの heap を解放して `Task.parallel` の worker のトラップも返す。再帰するプログラムの native 実行ファイルはスタック枯渇を `trap: stack overflow` で報告して `abort()` する（macOS と Linux で検証。Windows と `tsuzuri test` の実行ファイルは推定のまま） | [トラップ位置](tools/debugging.md) | [E14](../_features/_completed/E14-trap-boundary.md) |

## F 並列とバックエンド

| ID | 現在の提供範囲 | 解説 | 実装記録 |
| --- | --- | --- | --- |
| F01 | 遅延起動の常駐 pool、呼び出し元の参加、入れ子 | [Task runtime](language-reference/tasks.md) | [F01](../_features/_completed/F01-worker-pool.md) |
| F02 | init / map / map_ref / reduce / sum と固定チャンク | [Parallel](library-reference/parallel.md) | [F02](../_features/_completed/F02-data-parallel-api.md) |
| F03 | WASM SIMD128 の明示 opt-in、既定 fallback | [WASM SIMD](guides/webassembly.md) | [F03](../_features/_completed/F03-wasm-simd128.md) |
| F04 | 128-bit vector と mask、load、比較、順序付き還元 | [Simd](library-reference/simd.md) | [F04](../_features/_completed/F04-portable-simd-types.md) |
| F05 | native の同梱 i64 配列和に限る CPU dispatch | [CPU 選択](guides/performance.md) | [F05](../_features/_completed/F05-runtime-cpu-dispatch.md) |
| F06 | opt-in の共有メモリと Node Worker ホスト | [WASM threads](guides/wasm-threads.md) | [F06](../_features/_completed/F06-wasm-threads.md) |
| F07 | 実験的 CPU 参照、strict 整数 WGSL、WebGPU host | [GPU](guides/gpu.md) | [F07](../_features/_completed/F07-gpu-backend.md) |
| F08 | 未着手（計画）: 256-bit SIMD と関数単位の CPU 多版化 | [Simd](library-reference/simd.md) | [F08](../_features/F08-wide-simd-multiversioning.md) |
| F09 | 未着手（計画）: strict な float / 64-bit GPU カーネルと実行時接続 | [GPU](guides/gpu.md) | [F09](../_features/F09-gpu-float-runtime.md) |
| F10 | 未着手（計画）: Atomic / Mutex / Channel とスコープ付き並列 | [Task](language-reference/tasks.md) | [F10](../_features/F10-concurrency-primitives.md) |
| F11 | 対応（Phase 1・2）: WASM の build・test で `--wasm-max-memory`（wasm32 は最大 4 GiB − 64 KiB、wasm64 は 16 GiB）と `--wasm-stack-size`、root manifest の `[wasm]`、`--target wasm64`（memory64。Node.js 24 以降）。wasm32 の 2 GiB 超と threads は stack 溢れを入口で検査。既定は 16 MiB・1 MiB のまま。wasm64 の threads は対象外 | [WASM](guides/webassembly.md) | [F11](../_features/_completed/F11-wasm-memory-limit.md) |
| F12 | 対応（Phase 0・1）: 配列の添字が `0 .. len - 1` のループ、定数長、範囲を確かめた `if` の then 側で必ず範囲内と証明できたときだけ境界検査を省く。`array_index_sum` の計測と形ごとの記録あり。Phase 2（ループの版分け、`while` の帰納変数、`assert` による支配）は、可変の上限・先行する検査・`while` の形も `-O3` で LLVM が検査を落として同じ速さになる実測のため実装しない | [性能](guides/performance.md) | [F12](../_features/_completed/F12-bounds-check-elimination.md) |
| F13 | 未着手（計画）: ホスト提供の allocator、確保統計、freestanding 出力 | [C ABI](guides/native-interop.md) | [F13](../_features/F13-custom-allocators.md) |

## G 開発ツール

| ID | 現在の提供範囲 | 解説 | 実装記録 |
| --- | --- | --- | --- |
| G01 | runtime IR の同梱と追跡漏れ検査 | [ビルド](tools/build-and-cache.md) | [G01](../_features/_completed/G01-runtime-ir-tracking.md) |
| G02 | 複数診断、安定順、上限、JSON lines | [診断](tools/diagnostics.md) | [G02](../_features/_completed/G02-multiple-diagnostics.md) |
| G03 | 未使用・到達不能警告と deny-warnings | [警告](tools/diagnostics.md) | [G03](../_features/_completed/G03-warnings.md) |
| G04 | trap 理由、source site、side table | [トラップ位置](tools/debugging.md) | [G04](../_features/_completed/G04-trap-locations.md) |
| G05 | AST を維持する保守的 formatter | [fmt](tools/editor-tools.md) | [G05](../_features/_completed/G05-formatter.md) |
| G06 | test 宣言、filter、隔離、native / WASM | [テスト](tools/testing.md) | [G06](../_features/_completed/G06-test-runner.md) |
| G07 | stdio LSP、全量同期、診断、hover、definition、symbols | [LSP](tools/editor-tools.md) | [G07](../_features/_completed/G07-lsp.md) |
| G08 | DWARF と WASM debug sections、macOS sidecar | [デバッグ情報](tools/debugging.md) | [G08](../_features/_completed/G08-debug-info.md) |
| G09 | 宣言の doc comment、公開 API Markdown、hover | [文書生成](tools/documentation.md) | [G09](../_features/_completed/G09-doc-comments.md) |
| G10 | Windows MSVC 実装あり。Windows 実行ゲート未確認で blocked | [Windows](tools/build-and-cache.md) | [G10](../_features/G10-windows.md) |
| G11 | フロントエンド処理後の whole-build artifact cache | [cache](tools/build-and-cache.md) | [G11](../_features/_completed/G11-incremental-build.md) |
| G12 | 対応（Phase 1）: LSP の参照・rename・workspace symbol・補完・signature help・semantic tokens・quick fix・整形。暗黙の複製の inlay hint は A15 Phase 2。推論型・借用の inlay hints は計画 | [LSP](tools/editor-tools.md) | [G12](../_features/_completed/G12-lsp-extensions.md) |
| G13 | 未着手（計画）: REPL と単一ファイルのスクリプト実行 | [CLI](tools/command-line.md) | [G13](../_features/G13-repl.md) |
| G14 | 対応（Phase 1）: コンパイラ・Clang・LLD・SDK/libc をまとめた CLI 配布物、`toolchain info`。署名・公証・Release 公開は計画、Windows は未検証 | [ビルド](tools/build-and-cache.md) | [G14](../_features/_completed/G14-toolchain-distribution.md) |
| G15 | 未着手（計画）: クロスコンパイルと対応ターゲットの階層 | [ビルド](tools/build-and-cache.md) | [G15](../_features/G15-platform-targets.md) |
| G16 | 未着手（計画）: LLDB の型表示、PDB | [デバッグ](tools/debugging.md) | [G16](../_features/G16-debugger-experience.md) |
| G17 | 未着手（計画）: 並列コード生成、モジュール単位の解析キャッシュ、上限の見直し | [cache](tools/build-and-cache.md) | [G17](../_features/G17-incremental-compilation.md) |
| G18 | 未着手（計画）: bench 宣言、カバレッジ、プロパティテスト | [テスト](tools/testing.md) | [G18](../_features/G18-bench-coverage.md) |
| G19 | 未着手（計画）: edition、非推奨の警告、API 差分の検査 | [パッケージ](language-reference/modules-and-packages.md) | [G19](../_features/G19-editions-compatibility.md) |
| G20 | 対応: 関数内の型・所有権エラー回復、二次エラーの抑制 | [診断](tools/diagnostics.md) | [G20](../_features/_completed/G20-error-recovery.md) |

## 特に注意する未対応範囲

- 戻り値・ローカルに置く region 付き関数値型、region 間の outlives 制約、排他借用フィールド。
- HKT 型別名、高階 kind 引数、標準 Functor / Monad の自動導入。
- try / catch / finally、外部キャンセルトークン、開始済み Task の強制停止。
- locale 書式、書記素幅、一般的な Unicode 正規化、可変スライス。
- registry / git 依存、lockfile と版解決、ネットワーク取得、build script。
- 任意関数の実行時 CPU dispatch、SVE / SVE2、GPU 自動 offload、float / 64-bit WGSL。
- ブラウザー向け本番 threads glue、LSP completion / rename / formatting。
- Windows の実機実行検証、Windows の native での OS API（`E2002`）、PDB、ARM64 Windows、MinGW。

F# の class / 継承 / 型プロバイダー、.NET runtime、GC、REPL、GUI / ネットワーク API を提供する言語ではありません。標準の OS API はファイル・環境・時刻・乱数・プロセスに限られ、それ以外の外部機能はホストに置きます。

これらの多くは、各表の「未着手（計画）」のチケットで改善を計画しています。REPL、ネットワーク API などの計画も、実装されるまでは上記のとおり未提供です。対応表と方針は[チケット一覧の第2期](../_features/README.md#第2期-他言語比較で見える劣位の改善計画)にあります。

## 根拠と読み方

利用方法は本ドキュメント、厳密な契約は[言語仕様](../docs/language.md)、処理系の不変条件は[アーキテクチャ](../docs/architecture.md)、履歴は各チケットを参照してください。チケット内の着手前設計や古い「現状」を、そのまま現在の提供機能とは見なしません。
