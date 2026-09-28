# 対応状況と機能別索引

[ドキュメントのトップ](README.md)

対象は Tsuzuri 0.1.0、2026-09-28 時点のリポジトリです。機能チケット全55件について、現在の提供範囲と利用者向けの説明を対応付けています。チケットの done は、そのチケットで合意した段階の完了であり、当初の設計案の全項目や他言語との互換性を意味しません。

## 基礎言語

チケット化以前の機能も言語リファレンスに含めます。[字句とインデント](language-reference/lexical-and-layout.md)、[値と定数](language-reference/values-and-constants.md)、[関数と再帰](language-reference/functions.md)、[型と推論](language-reference/types.md)、[数値](language-reference/numbers.md)、[評価順序](language-reference/expressions-and-operators.md)、[所有権](language-reference/ownership.md)、[制御構文](language-reference/control-flow.md)が入口です。

## A 型システム

| ID | 現在の提供範囲 | 解説 | 実装記録 |
| --- | --- | --- | --- |
| A01 | 型適用、ジェネリックレコード、具体化ごとの所有権 | [レコード](language-reference/records.md) | [A01](../_features/A01-generic-records.md) |
| A02 | union、payload、列挙型、case 関数値 | [union](language-reference/unions.md) | [A02](../_features/A02-union-types.md) |
| A03 | match の網羅性エラーと到達不能節の警告 | [パターン](language-reference/patterns.md) | [A03](../_features/A03-match-exhaustiveness.md) |
| A04 | 有限値を持つ再帰型、所有ノード、反復 clone / drop | [再帰型](language-reference/unions.md) | [A04](../_features/A04-recursive-types.md) |
| A05 | 透過的な型別名、汎用別名、循環検査 | [型別名](language-reference/records.md) | [A05](../_features/A05-type-aliases.md) |
| A06 | 条件付き instance、superclass、default method | [型クラス](language-reference/generics-and-typeclasses.md) | [A06](../_features/A06-typeclass-extensions.md) |
| A07 | Eq / Ord / Display / Hash / Default の導出 | [deriving](language-reference/deriving.md) | [A07](../_features/A07-deriving.md) |
| A08 | UTF-16 char と Unicode scalar の utf8char | [文字](language-reference/strings-and-characters.md) | [A08](../_features/A08-char-type.md) |
| A09 | 共有借用フィールドと単一 region の名前付き契約 | [lifetime](language-reference/lifetimes.md) | [A09](../_features/A09-named-lifetimes.md) |
| A10 | 明示 kind、rank-1 HKT、末尾固定の部分適用 | [高階型](language-reference/higher-kinds.md) | [A10](../_features/A10-higher-kinded-types.md) |
| A11 | 比較の非消費化、借用 Eq / Ord、構造比較 | [比較クラス](language-reference/generics-and-typeclasses.md) | [A11](../_features/A11-borrowed-comparisons.md) |

## B エラー処理と制御

| ID | 現在の提供範囲 | 解説 | 実装記録 |
| --- | --- | --- | --- |
| B01 | Option / Result の型と所有・借用 API | [Option / Result](library-reference/option-result.md) | [B01](../_features/B01-option-result.md) |
| B02 | Option / Result ビルダーで失敗時に継続を短絡 | [計算式](language-reference/computation-expressions.md) | [B02](../_features/B02-result-propagation.md) |
| B03 | 通常ループの break / continue と解放 | [ループ](language-reference/control-flow.md) | [B03](../_features/B03-break-continue.md) |
| B04 | bool / Option の部分認識器、明示 union の複数 case | [アクティブパターン](language-reference/active-patterns.md) | [B04](../_features/B04-active-pattern-extensions.md) |
| B05 | match! / and!、BindReturn / Bind2。use / try は未対応 | [計算式の合成](language-reference/computation-expressions.md) | [B05](../_features/B05-computation-expression-extensions.md) |
| B06 | parallel_results の未開始停止と最小 index の Error | [Task](language-reference/tasks.md) | [B06](../_features/B06-task-cancellation.md) |

## C コレクションとデータ

| ID | 現在の提供範囲 | 解説 | 実装記録 |
| --- | --- | --- | --- |
| C01 | Array / List の消費的更新、一意領域の再利用 | [所有更新](library-reference/arrays-and-lists.md) | [C01](../_features/C01-consuming-update.md) |
| C02 | 非 Copy の Vec、容量管理、Array との移送 | [Vec](library-reference/vec.md) | [C02](../_features/C02-growable-vec.md) |
| C03 | 配列の半開共有スライスと寿命検査 | [スライス](library-reference/arrays-and-lists.md) | [C03](../_features/C03-slices.md) |
| C04 | 借用 bulk API、安定 sort、検索、fold、集計 | [Array / List](library-reference/arrays-and-lists.md) | [C04](../_features/C04-bulk-array-api.md) |
| C05 | 所有レコードのフィールド更新 | [レコード更新](language-reference/records.md) | [C05](../_features/C05-record-update-syntax.md) |
| C06 | 不透明な順序付き Map / Set、非 Copy キーの借用 | [Map / Set](library-reference/map-set.md) | [C06](../_features/C06-map-set.md) |
| C07 | 一回消費 Seq、明示 iter、借用要素の反復 | [Seq](library-reference/sequences.md) | [C07](../_features/C07-iteration-protocol.md) |

## D 文字列と数値

| ID | 現在の提供範囲 | 解説 | 実装記録 |
| --- | --- | --- | --- |
| D01 | Display / Parse、最短往復表示、所有文字列化 | [表示と解析](library-reference/formatting-and-parsing.md) | [D01](../_features/D01-display-parse-format.md) |
| D02 | 検索、分割、連結、切り出し、ASCII 操作、符号化・移送 | [文字列 API](library-reference/text.md) | [D02](../_features/D02-string-library.md) |
| D03 | 全 Float の基本演算、f32 / f64 の超越関数 | [Math](library-reference/math.md) | [D03](../_features/D03-generic-math.md) |
| D04 | checked / saturating、bit、rotate、拡大乗算 | [Int](library-reference/integers.md) | [D04](../_features/D04-integer-intrinsics.md) |
| D05 | 明示 FMA、固定木、補償和、順序付き内積 | [数値集計](library-reference/math.md) | [D05](../_features/D05-fma-ordered-reductions.md) |
| D06 | 具体型 const、前方参照、限られた式の評価 | [定数](language-reference/values-and-constants.md) | [D06](../_features/D06-compile-time-constants.md) |

## E モジュールとホスト

| ID | 現在の提供範囲 | 解説 | 実装記録 |
| --- | --- | --- | --- |
| E01 | private、公開型からの漏れ検査 | [可視性](language-reference/modules-and-packages.md) | [E01](../_features/E01-visibility.md) |
| E02 | std 埋め込み、予約名、未使用コードの除去 | [標準モジュール](language-reference/modules-and-packages.md) | [E02](../_features/E02-standard-library-infrastructure.md) |
| E03 | ディレクトリ階層と再帰的ソース探索 | [階層モジュール](language-reference/modules-and-packages.md) | [E03](../_features/E03-hierarchical-modules.md) |
| E04 | 限定 manifest とローカル path 依存 | [パッケージ](language-reference/modules-and-packages.md) | [E04](../_features/E04-packages.md) |
| E05 | 借用入力、所有バッファ結果、スカラーレコード | [C ABI](guides/native-interop.md)、[WASM](guides/webassembly.md) | [E05](../_features/E05-host-abi-buffers.md) |
| E06 | 同期 extern、到達する import のみ生成 | [外部関数](guides/native-interop.md) | [E06](../_features/E06-host-imports.md) |
| E07 | Debug.print / trace、native stderr と WASM opt-in | [Debug](tools/debugging.md) | [E07](../_features/E07-debug-output.md) |

## F 並列とバックエンド

| ID | 現在の提供範囲 | 解説 | 実装記録 |
| --- | --- | --- | --- |
| F01 | 遅延起動の常駐 pool、呼び出し元の参加、入れ子 | [Task runtime](language-reference/tasks.md) | [F01](../_features/F01-worker-pool.md) |
| F02 | init / map / map_ref / reduce / sum と固定チャンク | [Parallel](library-reference/parallel.md) | [F02](../_features/F02-data-parallel-api.md) |
| F03 | WASM SIMD128 の明示 opt-in、既定 fallback | [WASM SIMD](guides/webassembly.md) | [F03](../_features/F03-wasm-simd128.md) |
| F04 | 128-bit vector と mask、load、比較、順序付き還元 | [Simd](library-reference/simd.md) | [F04](../_features/F04-portable-simd-types.md) |
| F05 | native の同梱 i64 配列和に限る CPU dispatch | [CPU 選択](guides/performance.md) | [F05](../_features/F05-runtime-cpu-dispatch.md) |
| F06 | opt-in の共有メモリと Node Worker ホスト | [WASM threads](guides/wasm-threads.md) | [F06](../_features/F06-wasm-threads.md) |
| F07 | 実験的 CPU 参照、strict 整数 WGSL、WebGPU host | [GPU](guides/gpu.md) | [F07](../_features/F07-gpu-backend.md) |

## G 開発ツール

| ID | 現在の提供範囲 | 解説 | 実装記録 |
| --- | --- | --- | --- |
| G01 | runtime IR の同梱と追跡漏れ検査 | [ビルド](tools/build-and-cache.md) | [G01](../_features/G01-runtime-ir-tracking.md) |
| G02 | 複数診断、安定順、上限、JSON lines | [診断](tools/diagnostics.md) | [G02](../_features/G02-multiple-diagnostics.md) |
| G03 | 未使用・到達不能警告と deny-warnings | [警告](tools/diagnostics.md) | [G03](../_features/G03-warnings.md) |
| G04 | trap 理由、source site、side table | [トラップ位置](tools/debugging.md) | [G04](../_features/G04-trap-locations.md) |
| G05 | AST を維持する保守的 formatter | [fmt](tools/editor-tools.md) | [G05](../_features/G05-formatter.md) |
| G06 | test 宣言、filter、隔離、native / WASM | [テスト](tools/testing.md) | [G06](../_features/G06-test-runner.md) |
| G07 | stdio LSP、全量同期、診断、hover、definition、symbols | [LSP](tools/editor-tools.md) | [G07](../_features/G07-lsp.md) |
| G08 | DWARF と WASM debug sections、macOS sidecar | [デバッグ情報](tools/debugging.md) | [G08](../_features/G08-debug-info.md) |
| G09 | 宣言の doc comment、公開 API Markdown、hover | [文書生成](tools/documentation.md) | [G09](../_features/G09-doc-comments.md) |
| G10 | Windows MSVC 実装あり。Windows 実行ゲート未確認で blocked | [Windows](tools/build-and-cache.md) | [G10](../_features/G10-windows.md) |
| G11 | フロントエンド処理後の whole-build artifact cache | [cache](tools/build-and-cache.md) | [G11](../_features/G11-incremental-build.md) |

## 特に注意する未対応範囲

- 独立した複数 record region、region を保持する関数値型、排他借用フィールド。
- HKT 型別名、高階 kind 引数、標準 Functor / Monad の自動導入。
- use / try / catch / finally、外部キャンセルトークン、開始済み Task の強制停止。
- 文字列補間、locale 書式、一般的な Unicode 正規化、可変スライス、HashMap。
- registry / git 依存、lockfile と版解決、ネットワーク取得、build script。
- 任意関数の実行時 CPU dispatch、SVE / SVE2、GPU 自動 offload、float / 64-bit WGSL。
- ブラウザー向け本番 threads glue、LSP completion / rename / formatting。
- Windows の実機実行検証、PDB、ARM64 Windows、MinGW。

F# の class / 継承 / 型プロバイダー、.NET runtime、GC、REPL、標準 OS / GUI / ネットワーク API を提供する言語ではありません。外部機能はホストに置きます。

## 根拠と読み方

利用方法は本ドキュメント、厳密な契約は[言語仕様](../docs/language.md)、処理系の不変条件は[アーキテクチャ](../docs/architecture.md)、履歴は各チケットを参照してください。チケット内の着手前設計や古い「現状」を、そのまま現在の提供機能とは見なしません。
