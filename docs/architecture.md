# コンパイラ構成

実装言語には Rust を採用しています。2進（binary）リテラルの正確な丸めには `rustc_apfloat` を用い、LLVM の C API や Rust バインディングとは密結合させません。
テキスト形式の LLVM IR と、バージョン差の比較的小さい Clang／LLD の CLI をシステム境界として利用しているため、`cargo build` でコンパイラをビルドする際に LLVM 開発用ヘッダーや CMake は不要です。

```text
UTF-8 .tz / .tt / .tc files below one project root (application entry: root/Main.tz)
   -> driver -> sorted source files + filename-based module names and source kinds
             + the root package's default namespace for its sources
             + embedded std sources (stdlib::SOURCES) appended after user sources
   -> lexer -> tokens + per-file byte spans
   -> parser -> syntax AST per file
   -> check -> source kinds + builder expansion + module-scoped names
            -> rigid type variables + local unification
            -> match coverage: exhaustiveness errors + unreachable-arm warnings
   -> ownership -> symbolic moves/loans + inferred Copy requirements
   -> polymorph -> constraint fixed point + coherent instances + monomorphization
   -> closures -> lambda lifting + capture parameters + saturated export bridges
               + FunctionOrigin of every generated helper
   -> ownership -> concrete moves + loans + lifetime validation
   -> llvm -> reachability pruning of std code + bounded known-call specialization
           + deterministic LLVM IR
   -> Clang -O0..3 -> native executable / PIC object
                   -> wasm32 object -> LLD -> standalone .wasm
```

| ファイル | 責務 |
|---|---|
| `src/diagnostic.rs` | ソース ID とファイル内位置、診断の順序・重複除去・表示／収集上限、human／JSON lines の出力制御 |
| `src/syntax.rs` | トークン定義、構文木（AST）、構文リソース上限の管理 |
| `src/lexer.rs` | UTF-8 を壊さない字句走査、コメント処理（ファイル先頭の `#!` 行も行コメントとして飛ばし、位置はファイル全体のまま。`shebang_length`）、数値リテラルの切り出し |
| `src/docgen.rs` | 宣言 AST からの公開 API ドキュメント（Markdown）生成、型・制約・region の描画、決定的なページソート |
| `src/parser.rs` | Pratt パーサー、宣言と式、トップレベルのエントリーコード、再帰深度の制限 |
| `src/parse_control.rs` | インデントによるブロック構文、for／while／match、関数ガード、ラムダ式、パターンおよびアクティブパターン認識器名 |
| `src/check.rs` | 全モジュールのシグネチャ収集、名前解決、型付き IR の構築、メモリレイアウト計算、公開 ABI |
| `src/control.rs` / `recursion.rs` | 型付きループ、短絡評価を行うパターン手順と束縛、認識器呼び出し、参照グラフに基づく再帰検査 |
| `src/exhaustiveness.rs` | 型付きの被覆パターン、usefulness アルゴリズムによる match の網羅性検査・到達不能節の検出、不足パターンの提示（check の子モジュール） |
| `src/computation.rs` | ソース種別の検査、`.tc` ビルダーの収集、型検査前の関数・継続への展開（check の子モジュール） |
| `src/polymorph.rs` | 型変数の単一化、型クラス・インスタンス、モジュール関数制約の解決、制約の伝播、単相化（check の子モジュール） |
| `src/closures.rs` | 匿名関数の検査、自由変数の捕捉解析、lambda lifting、公開 ABI 向けの完全適用ラッパー生成 |
| `src/numeric.rs` | プリミティブ型名、整数・浮動小数点接尾辞、binary／decimal リテラルの丸めとエンコーディング |
| `src/constants.rs` | 定数の依存順評価、型ごとの演算とリソース上限、既存リテラルへの展開、定数・一時値の借用引数管理（`temporary_borrows`） |
| `src/exceptions.rs` / `src/llvm_exception.rs` / `std/Exception.tz` | `try ... with ... finally`・`@checked`・単項 `+` の型検査（check の子モジュール）と lowering、`**` の lowering、std の `Exception`／`ExceptionKind` と `Err` の instance 実装 |
| `std/BigInt.tz` | `bigint` 実装。符号と、下位から並べた $10^9$ 進の桁（`[i64]`）による標準ライブラリ（std）ソースのみの多倍長整数（不透明 record）。コンパイラは型名 `bigint` を `BigInt` に、`I` 接尾辞のリテラルを桁配列を渡す `BigInt.make` 呼び出しへ展開するのみ |
| `src/ownership.rs` | 部分 move、借用の競合、最後の使用位置の特定、分岐の合流、参照の生存期間（lifetime）検証 |
| `src/ownership_control.rs` | ループ反復の不動点解析、ガード節の読み取り専用別名、分岐・認識器の一時値の寿命管理 |
| `src/llvm.rs` | SSA 形式への変換、phi ノード、末尾再帰のループ化、所有値の解放、借用追跡、ホスト呼び出しラッパー、C ヘッダー生成 |
| `src/llvm_debug.rs` | 共通採番による DWARF メタデータ生成、型・変数・関数と式のソース位置情報の付与 |
| `src/llvm_imports.rs` | extern 関数の ABI ラッパー生成、リンク名と WASM import 属性、コールバック引数、所有結果の受領時検証 |
| `src/bindings.rs` / `src/runtime/bindings-core.mjs` / `bindings.mjs` / `bindings-threads.mjs` | 公開 ABI の記述子の表（export・到達する import・record の C 配置）と、それを読む型付きホスト バインディングの生成（E13）。`--emit bindings-js` は表と固定ランタイム（共通部 + 1 スレッドの `load` か、`--wasm-feature threads` の Web Worker プール）を連結した ES module と `.d.mts` |
| `src/bindings_native.rs` | 同じ型モデルから C#（`[LibraryImport]`・`SafeHandle`）、Python（`ctypes`）、C++20（C ヘッダーの上の RAII）のバインディングを生成する（E13 Phase 2。`bindings.rs` の子モジュール） |
| `std/IO.tc` / `src/llvm_io.rs` / `src/runtime/io.c` | 不透明な IO モナド、エントリーポイントでの実行、標準入出力ストリーム、WASM ホスト境界 |
| `src/runtime/arguments.c` | `def main :: Array<string> -> i32` 向けコマンドライン引数。POSIX／WASI における UTF-8 デコード（不正なバイト列は U+FFFD に置換）および Windows のコマンドライン分割 |
| `std/Os.tz` / `File.tz` / `Dir.tz` / `Path.tz` / `Env.tz` / `Time.tz` / `Random.tz` / `Process.tz` / `src/runtime/os.c` / `src/runtime/os-wasi.c` | OS API。純粋な std ソース、`Os.__*` 組み込み関数（`src/llvm_io.rs` の `os_builtin`）、POSIX ランタイム、`--wasm-host wasi` 向けの WASI preview1 ランタイム |
| `std/HashMap.tz` / `std/HashSet.tz` | ハッシュコンテナ。コンパイラ本体に専用の型・builtin・ランタイムを追加しない、std ソースのみによる実装 |
| `std/Regex.tz` / `std/Unicode.tz` / `src/runtime/unicode.ll` / `scripts/generate-unicode.mjs` | 線形時間の正規表現（std ソースの Pike VM）と Unicode の表。表は UCD 17.0.0 から生成したランタイム定数で、std 専用の組み込み `Unicode.__table_length`／`__table_entry` が読む |
| `std/Json.tz` | JSON（RFC 8259）の解析・出力、`Json.Value`、組み込みクラス `Encode`／`Decode` の std インスタンスと導出用の補助関数。コンパイラは 2 クラスの登録（`Classes::collect`、シグネチャは std の `Json.Value`／`Json.Error`／`Result` から引き、std に無ければ使用時に `E1004`）と導出（`src/derive.rs`）だけを持ち、専用の `Type`・builtin・ランタイムはない |
| `std/Cbor.tz` | `Json.Value` の CBOR（RFC 8949）。決定的な符号化と、JSON のデータモデルに限った厳密な復号。浮動小数点のビット列は 2 のべきの厳密な拡大・縮小で求め、専用の builtin を使わない |
| `std/Format.tz` / `src/runtime/format.ll` | 文字列補間の書式指定。`Format.parse`／`Format.pad` と、パディング処理のランタイム補助（文字列結合は `src/llvm_display.rs`） |
| `src/simd.rs` / `src/llvm_simd.rs` | 128-bit・256-bit の vector/mask 型、lane 型族、境界検査、LLVM vector への lowering、256-bit の load／store の `align 16` |
| `src/llvm_cpu.rs` | `@cpu` 関数の level ごとの版、版を選ぶ stub、256-bit ベクトルを渡す呼び出し先の版（F08 Phase 3） |
| `src/llvm_control.rs` | 直接反復・switch・定数テーブル生成、パターンマッチ手順の分岐展開と全経路での解放処理 |
| `src/llvm_bulk.rs` | 配列連結・リストの一括走査・安定なマージソート（merge sort）の型付き builtin lowering |
| `src/llvm_compare.rs` | 配列・リスト・タプルの借用構造比較、短絡評価と段階的な比較メソッド適用 |
| `src/derive.rs` / `src/llvm_hash.rs` / `src/llvm_display.rs` | 導出（derive）instance の AST 合成、canonical Hash、引用符・コレクション表示、文字列補間（`Interpolated`）の一括結合 |
| `src/llvm_math.rs` / `src/runtime/math.c` / `src/runtime/musl/` | Float 基本数学演算と型別 Elementary 関数、固定バージョンの移植可能な数学関数実装 |
| `src/recursive.rs` / `src/llvm_recursive.rs` / `src/runtime/recursive.ll` | 具体型ごとの再帰成分、所有ノード、追加確保を伴わないスタックレス解放と反復複製 |
| `src/llvm_frame.rs` | `new` を伴わないリテラル向けのスタックフレーム領域確保、実行時アドレス判定、スコープ外移動時のヒープ移送、フレームを考慮した解放 |
| `src/call_specialization.rs` | エスケープしない関数引数の不動点解析、既知の継続・読み取り専用捕捉の判定、LLVM worker 関数の特殊化予算管理 |
| `src/ranges.rs` | 型付き IR 上での配列添字の範囲証明（規則 R1–R5）と関数単位の `RangeFacts`。証明された添字のみ境界検査の分岐を省略 |
| `src/runtime/numeric.c` / `numeric.ll` | 多倍長整数を用いた f16／f128／decimal 演算、比較、広幅・異形式間の変換、最短往復文字列表現・解析、書式指定付き数値表示（`tz_soft_format_spec`） |
| `src/runtime/string.ll` / `utf8string.ll` / `heap-*.ll` | UTF-16／UTF-8 バッファ操作・明示的な文字符号化変換、ネイティブメモリ確保、WASM の再利用・結合可能なヒープ管理 |
| `src/runtime/closure.ll` | 関数値の環境の複製および解放。環境ごとの固有処理は LLVM emitter が生成 |
| `src/runtime/cpu.c` | 整数 8 型の配列の和・最小・最大の level ごとの kernel、CPUID/OSXSAVE/XCR0 と `getauxval` による機能検出、アトミックな level キャッシュ、`@cpu` 関数の版を選ぶ `tsuzuri_cpu_pick` |
| `src/runtime/heap-*.ll` の `tz.realloc` | ネイティブの realloc、および WASM における隣接空き領域の再利用と確保・コピーへのフォールバック |
| `src/runtime/task.c` / `task-wasm.ll` / `task-wasm-threads.c` | ネイティブ常駐スレッドプール、WASM デフォルトの逐次実行、opt-in の共有メモリ Worker プール |
| `src/runtime/wasm.ll` | 128-bit 乗除算・剰余・ビットシフトの freestanding 補助関数群 |
| `src/stdlib.rs` / `std/` | 埋め込み標準ライブラリのソースコード、予約 std モジュール名、std の仮想パス解決 |
| `src/driver.rs` | ソースファイルの列挙、`Main.tz` の選択、LLVM／LLD の起動、ステージング、出力保護。ツールは `TSUZURI_*` → 配布物（実行ファイルの2階層上に `manifest.json`）の `bin/` → `PATH` の優先順で解決（`resolve_tool`。キャッシュキーにも同一の解決ロジックを使用） |
| `src/main.rs` | CLI オプションの解析と診断・警告の表示、`toolchain info`、`fetch`、`publish`、`bindgen`、`repl` のオプション、`script` の引数の取り分け（ファイルより前を `run` のオプションとして解析し、後ろをプログラムへ渡す） |
| `src/package.rs` / `src/fetch.rs` | マニフェストの限定 TOML、版（`Version`）、`Tsuzuri.lock` と registry index の厳密な JSON と正規形、内容ハッシュ、`tsuzuri fetch` による git・registry 依存の取得・最小版選択・検査・ストアへの確定、`tsuzuri publish`（`git` を起動する唯一の経路） |
| `src/bindgen.rs` / `src/bindgen_driver.rs` | `tsuzuri bindgen`（E11）。前者は Clang の JSON AST から宣言の所属ファイルを追跡し、型の表記を typedef 展開して固定の表と完全一致で照合し、名前の規則と `W2002` の理由を適用して決定的なテキストを作る純関数だけを持つ。後者（driver の子モジュール）はヘッダーの読み込みと SHA-256、Clang の起動（stdout は 256 MiB まで、stderr は別スレッドで読む）、LP64 の確認、出力保護とステージングを伴う書き込みを行う |
| `src/copies.rs` | 具体化後の暗黙の複製箇所の列挙（`copies::sites`）、`--warn implicit-copy` による `W1006` 警告、インレイヒント（inlay hint）の基となる配列・リストの複製検出（`costly_sites`） |
| `src/lsp.rs` / `src/semantic.rs` | stdio 経由の言語サーバー、Unicode 位置変換、単相化前の型・定義位置インデックス。定義・参照・ローカル変数の有効範囲・record 型の式をインデックス化し、型付き木で脱落するフィールド名・record 名・case 名は checker の `name_uses` から収集。リネームとクイックフィックスは編集後の再解析により診断と名前の結び付きの不変性を検証。入力中の補完・シグネチャヘルプ・セマンティックトークン・複製のインレイヒントは、直前の成功インデックスを共通の接頭辞・接尾辞に基づいて写像して再利用 |
| `src/repl.rs` | `tsuzuri repl`（G13）。入力の読み取り（1 行目が末尾で未完なら空行まで継続）、宣言の塊（parser が各トップレベル宣言を始めた位置 `Program::declaration_starts` で区切る。列は問わず、文書コメントと属性はその宣言に入り、同じ行の前の空白とコメントも付く。`and` の再帰群と、別に書いたシグネチャと定義は一つの項目）とトップレベルの文（`;` で終わる文は `;` ごと残す）への分類、アクションの行をまとめて左へ寄せること、同じ名前の項目の置き換え、生成した `Main.tz` の解析と `SemanticIndex` からの型の取得、`driver::run_captured` による実行、診断の位置の `input`／`session` への写像 |

ドキュメントコメント（doc comment）は、字句解析器（lexer）において `DocComment` トークンとして保持され、構文解析器（parser）によって対応する宣言の `Documentation(text, span)` に付与されます。
`def` と `fn` を分離して書いた場合はシグネチャ側の記述から引き継がれ、不適切な位置への配置は `E0002` エラーとなります。生成される型検査済み関数（checked function）にはドキュメント文字列を複製しないため、型システム、所有権モデル、および LLVM のコード生成の意味論を変えることはありません。
`doc` コマンドは、通常のプロジェクト検査を完了した後に、ソース AST からシグネチャを描画します。再パースはドキュメント用の構文情報を取得するためだけに用いられ、独立した型推論や LLVM コード生成は行われません。
`private` 宣言や `instance` の実装を除き、同一の `.tz` および `.tc` を共有する標準モジュール（std module）は 1 ページにまとめて出力されます。定数（`const`）の初期化式は省略され、組み込み関数の一覧については言語仕様への参照が案内されます。
ドライバー（driver）によるドキュメントの公開処理では、マーカーファイル、ソースの包含関係、およびシンボリックリンクを厳密に検証したうえで、一時ディレクトリからディレクトリ全体をアトミックにリネームして配置します。旧出力の退避と失敗時の復元機構は、通常の `build` におけるファイル出力処理から完全に独立しています。
フォーマッタ（formatter）のフィンガープリント計算ではドキュメント本文を比較しつつ span 情報のみを除去します。`SemanticIndex` は宣言位置ごとに本文を 1 回だけ保持し、LSP のホバー表示は既存のターゲット span から説明テキストを取得します。

## 性能設計の原則

**最高峰の実用性能と安全性を両立させることは、コンパイラ・ランタイム・組み込み関数、そして将来の標準ライブラリ全体に共通する最重要の設計要件です。** 高水準で安全な API の内部では、ターゲット環境で利用可能な CPU 命令、SIMD、マルチコア CPU、GPU、および高度に最適化されたカーネルを積極的に活用します。
ただし、ハードウェア利用率やバックエンドの名称そのものは性能の証明にはなりません。真の目標は「正しい演算結果を得るまでの総所要時間の最小化」です。単に LLVM にコードを委ねるだけで高速化されたと即断せず、生成された機械語や IR、および厳密な実測によって最適化経路を検証してください。

現在の実装状況と今後の拡張要件は明確に区別して管理します。

| 項目 | 現在の実装／今後の要件 |
|---|---|
| スカラー CPU | i8〜i64／i8u〜i64u および f32／f64 間の型変換には、LLVM の直接命令および飽和（saturating）intrinsic を使用。キャスト演算子 `as` と組み込み変換関数の意味論および性能経路を完全に一致させる |
| SIMD | `-O3` におけるループおよび SLP 自動ベクトル化。メモリ上で連続した配列配置、型の特殊化、不要なコピーの徹底排除により、LLVM が最適化しやすい IR を生成。`--cpu native` 指定時にはビルドホスト固有の命令セットを有効化 |
| 移植性 | デフォルトの `--cpu generic` ではターゲットアーキテクチャのベースライン命令セットを採用。同梱の i64 配列和のみ実行時 ISA 選択を実施。`native` 指定時は配布バイナリの前提条件にビルド機の ISA が含まれる |
| 複数 CPU コア | `Task.parallel` 向けに遅延起動する常駐スレッドプールを実装。ハードウェアの CPU コア数に応じて追加スレッド数を適切に制限し、呼び出し元スレッド自身も自グループの処理を推進。WASM は逐次フォールバック。自動並列化は未実装 |
| GPU | 実験的なカーネル抽出、CPU 参照実装、厳密な整数 WGSL 出力、および WebGPU ホスト試作を実装。通常ランタイムへの実 GPU 自動接続、浮動小数点 GPU 演算、自動オフロードは未実装 |
| WASM | bulk-memory に標準対応。SIMD128、マルチスレッド（threads）、`--wasm-host wasi` はそれぞれ明示的なオプトイン（opt-in）制。デフォルトは SIMD なし・ホストインポートなし（IO は `tsuzuri_io`）・逐次実行であり、OS API の呼び出しは `E2000` で拒否 |

新しい組み込み関数や標準ライブラリを設計する際は、要素ごとの汎用的な関数呼び出しを基本実装とせず、要素型・メモリ連続性・データサイズを静的に把握できる一括操作（バルク操作）として設計してください。必ず厳密で正確な基準実装（リファレンス）を用意し、スカラー、SIMD、並列 CPU、GPU の各経路で同一の契約が満たされているかを検証します。既存の所有権・借用・不変性の情報から別名関係（エイリアス）の不在や独立性を論理的に証明できる場合にのみ最適化を適用し、根拠のない `noalias` 属性などを安易に付与してはなりません。

バックエンドの自動選択にあたっては、メモリ確保、コンパイル・初期化、ディスパッチ、データ転送、同期、および結果の回収に至るすべてのオーバーヘッドを含めた損益分岐点（閾値）を、実機上で計測して決定します。小規模な処理を漫然と GPU にオフロードしたり、スレッド数を増やせば無条件に高速化すると仮定したりしてはいけません。GPU 常駐データについてはホスト・デバイス間の往復転送を避ける設計を徹底します。また、GPU 等に対応していない環境でも言語契約を維持できる CPU フォールバック経路を提供し、実際に選択された実行経路を観測可能にします。GPU 実行を明示的に要求したにもかかわらず環境が未対応である場合や実行に失敗した場合は明確に診断エラーを報告し、暗黙のうちに成功扱いにしたり結果の精度を改変したりしてはなりません。

高速化を目的として、整数のオーバーフロー（折り返し）、飽和演算、最近接偶数丸め、NaN の扱い、符号付きゼロ、式の評価順序、トラップ挙動を変更することは禁止します。浮動小数点の集約順序の変更や FMA（積和演算）の融合が必要な場合は、まず独立した別名 API や明示的なモードとしての契約を策定し、既存の演算子へ暗黙的に適用してはなりません。新設した最適化経路には、境界値テストおよび参照実装との照合テストを用意し、同一条件下での C/C++ との比較検証を実施してください。共有 CI では意味論の正しさとリグレッション（性能退行）の有無を検査し、厳密な性能閾値の判定はノイズの少ない専用環境で行います。

WebAssembly の機能フラグ（WASM feature）は `BuildOptions.wasm_simd` および `wasm_threads` で表現されます。ドライバーは SIMD 有効時に `-msimd128`、デフォルトでは `-mno-simd128` を Clang に渡します。生成される LLVM IR には feature 要件がコメントとして記録されます。`tests/wasm_simd.mjs` は `llvm-objdump` の逆アセンブル結果と `BigInt` 参照実装を用いて検証を行い、即値の `0xfd` を SIMD オペコードと誤認しないように厳密に命令解析を実施します。

マルチスレッド対応（threads）は WASM およびオブジェクト出力専用の機能です。C11 の freestanding な `task-wasm-threads.c` を `-matomics` および `-mbulk-memory` でコンパイルし、`wasm-ld` の `--shared-memory` および `--import-memory` を用いて結合します。`heap-wasm.ll` 内のアロケータを内部名へ変更し、`heap-wasm-threads.ll` のロックラッパーから呼び出す設計とすることで、`realloc` 内部で行われるメモリ確保と解放も単一のロック内で安全に完結させています。
共有状態はリニアメモリ上に配置され、`wasm-ld` による一度限りのデータ初期化を利用します。各インスタンスの `__stack_pointer` とスタック範囲（`tsuzuri_stack_base`、`tsuzuri_stack_top`。メインスレッドは 0）のみをホスト側から設定します。
タスクグループは呼び出し元のスタック上に確保され、キューのロック内でアトミックにタスクを取得してからコールバックを実行します。残存タスク数（`remaining`）の公開後はグループを参照せず、呼び出し元はロック内でアンリンクして復帰します。
呼び出し元スレッド自身もタスクの実行を進め、入れ子になったタスクでは自グループを優先しつつ他グループの処理も手伝うワークスティーリング的な挙動をとります。エポックベースの wait/notify によってアイドル待機を行い、ヒープのロック内ではユーザーコールバックを決して呼び出さない設計としています。
Node.js 側のホスト実装である `src/runtime/wasm-threads.mjs` は、明示的に指定されたワーカー数を初期化し、初回グループ生成時に `spawn_workers` を呼び出します。各ワーカーの 256 KiB のスタックはヒープから確保され、クローズ時にはタスク完了を待ってから Worker を正常に終了します。
メモリ設定を省略した場合は、WASM バイト列のインポートセクションから `env.memory` の最大ページ数を読み取り、初期サイズ・最大サイズともにそのページ数で共有メモリを作成します（デフォルトは 256 ページであり、`WebAssembly.Module` を渡した場合も同様に 256 ページとなります）。
ワーカーでのトラップや初期化失敗が発生した際は、共有の失敗フラグ（`failed`）とロックのポイズンビットをセットしてすべての待機状態を解除します。以降のタスク実行は即座に拒否され、トラップ発生後のメモリ解放は保証されません。これは言語通常の `Result` によるエラーハンドリングとは明確に区別されます。
`tests/wasm_threads.mjs` では、`-O0`／`-O3` におけるスタック番兵（sentinel）、ワーカーのスタックオーバーフロー時のトラップ、アトミックバリア、ヒープ残量、ネスト実行、一括処理、障害発生時の動作、オブジェクト出力、ならびに SIMD／デバッグ情報との併用と決定性を包括的に検証しています。

`src/runtime/trap-boundary.mjs` は単一スレッド WASM 向けの同梱 JavaScript ホスト実装であり、コンパイラ本体からは参照されず、生成バイナリの挙動にも影響しません。`createBoundary(module, { imports, sites })` の呼び出しにより、1 回のエクスポート関数呼び出しを安全な境界で囲みます。
WebAssembly 内部の例外は、`WebAssembly.RuntimeError` の場合は `{ reason: "trap", site }` とサイドテーブルのソース位置を返し、V8 の `RangeError`（SpiderMonkey では `InternalError`）によるスタック枯渇の場合は `{ reason: "stack" }` として返します。ホストのインポート関数から送出された例外は、同一オブジェクトのまま再送出されます。
Tsuzuri はトラップ時にスタックの巻き戻しを行わないため、例外が発生したインスタンスは原因分類のために `tsuzuri_trap_site` を 1 回呼び出した後は二度と再利用せず、次回の呼び出し時には同一モジュールから新しくインスタンスを再生成します。マルチスレッド（threads）モジュールはこの境界で拒否され、スレッドプールの単位は既存の `createThreadPool` で管理されます。
`--emit bindings-js` のグルー（`src/bindings.rs` の表 + `src/runtime/bindings.mjs`）は、`load` で表の記述子を変換関数へ一度だけ解決し、呼び出しごとに記述子の文字列で分岐しません。境界の規則は `trap-boundary.mjs` と同じ（例外でインスタンスを捨て、`tsuzuri_trap_site` を 1 回だけ読み、次の呼び出しで同期的に作り直す）ですが、生成物は利用者が配布する単独のファイルなので、そのファイルを import せずに同じ規則を自前で持ちます。インスタンスごとに import の wrapper を作り、ホストの例外の記録、コールバックの有効期間、捨てたインスタンスへの再入の拒否をその単位で扱います。`tests/bindings.mjs` が `-O0`／`-O3`、`--trap-info`、`--allocator counting`（各呼び出し後の `live == 0`）の build と TypeScript 6.0.3 の型検査で検証します。同期の作り直しが失敗したとき（Chrome のメインスレッドは 8 MB を超えるモジュールの同期 instantiate を拒む）は呼び出しを `Error` で止め、`ready()` が `WebAssembly.instantiate` で非同期に作り直します。

`--wasm-feature threads` のグルー（`bindings-threads.mjs`）は、`src/runtime/wasm-threads.mjs` と同じプロトコルを Web Worker で行います。グルー自身を `?tsuzuri-worker=coordinator|helper` 付きの module worker として起動し、補助ワーカーは最初の呼び出しの前に instantiate して起動記録の SharedArrayBuffer で待ちます（待っているスレッドが作ったワーカーは、そのスレッドがイベントループに戻るまで起動しないため）。調整役のインスタンスが export を実行し、`spawn_workers` が各補助ワーカーのスタック範囲を起動記録に書いて起こします。ブラウザのメインスレッドは atomic wait できないので、ページ側の `exports` は引数を検査してから調整役へ送る `Promise` です。補助ワーカーの失敗は理由とサイト ID を起動記録に残し、`poison` でプールの待ちを解いて、調整役のトラップ分類がそのサイトを使います。プールは作り直しません。`crossOriginIsolated` でないページは、ワーカーを起動する前に `Error` です。`tests/bindings_threads.mjs` が Node.js 上の Web Worker の adapter と、任意で実ブラウザ（`TSUZURI_BROWSER`、`TSUZURI_PLAYWRIGHT`）で検証します。

`--emit shared` は、実行ファイルと同じ object（ランタイムとトラップのランタイムを含む）を `-dynamiclib`（macOS、`-exported_symbols_list`、install name は `@rpath/<file>`）か `-shared`（Linux、version script、soname、`--no-undefined`）でリンクし、公開 C ABI の名前だけを export します。export する名前は、生成 IR が定義する `tz_*`、`tsuzuri_alloc`、`tsuzuri_free`、`tsuzuri_main`、`tsuzuri_alloc_stats`、`tsuzuri_try_*` から決まります（`shared_exports`）。C#・Python・C++ のバインディング（`src/bindings_native.rs`）は LLVM を通さず、C ヘッダーと同じ `record_name`／`record_layout`／`handle_c_name` で型を名付けて配置を揃え、`tests/host_bindings.mjs` が .NET SDK、python3、`clang++ -std=c++20` で `-O0`／`-O3` と `--trap-mode return` を検証します。
ネイティブ環境では、`driver::probable_stack_exhaustion` が子プロセスの終了シグナル（SIGSEGV、SIGBUS）を検知してスタック枯渇の可能性を推定し、`E2005` またはテスト失敗の理由として報告します。`tests/trap_boundary.mjs` により、`-O0`／`-O3` における 17 のケースが検証されています。

ネイティブオブジェクトにおけるエラー復帰境界（E14 Phase 2）は、`--trap-mode return` によって有効化されます。これはネイティブ出力（object、llvm、header）でのみ受け付けられ、`--trap-info` を内包します。各エクスポート関数 `tz_name` に対して `int32_t tsuzuri_try_name(tsuzuri_trap_info *trap, 結果ポインタ, 引数...)`（ステータス 0: 成功、1: トラップ、2: 入れ子呼び出しエラー）が追加生成され、C ヘッダー（型定義の重複は `TSUZURI_TRAP_INFO_DEFINED` で防止）および IR のサンクが出力されます。
`setjmp` は `src/runtime/trap.c` の内部でのみ使用され、LLVM IR には一切現れません。`llvm_traps::instrument` が `tz.trap.report` の先頭に `tsuzuri_trap_raise(site, kind)` を挿入し、境界のスレッドローカルフレームが存在すれば `longjmp` で脱出します。境界が存在しない場合は従来どおり標準エラーへ報告したのち `llvm.trap` で終了します。
この境界下では、`heap-native.ll` 内の `malloc`、`realloc`、`free` が `tsuzuri_tracked_*` に置き換えられます。返されるポインタ自体は通常の `malloc` と同一ですが、確保の追跡情報は境界ごとに独立した双方向リストおよび拡張ハッシュテーブルで管理され、解放時にはこのテーブルから対象ポインタを検索します。追跡情報のメモリ確保に失敗した場合は獲得したポインタを解放して NULL を返し、`realloc` の失敗時には元のメモリ領域を保護します。トラップ発生時には追跡対象の全ブロックが一括して `free` されます（Tsuzuri コードは巻き戻らないためデストラクタ呼び出しやロック解除は行いませんが、ランタイムはコールバック呼び出し前にロックを手放す規約となっています）。正常終了時には追跡情報のみが破棄され、ホスト側が所有する結果バッファはそのまま残されます。
境界管理構造体（ロック、リスト、ハッシュテーブル）は `tsuzuri_boundary_run` の自動変数（ローカル変数）としては保持せず、追跡対象外の通常の `malloc` で確保して `volatile` ポインタとして保持します。これは、`setjmp` 以降に変更された非 `volatile` な自動変数が `longjmp` による復帰後に未定義値となり、コンパイラの最適化（`-O2` 以上で顕在化）によってメモリ解放処理が古いポインタを読み出してしまう不具合を防ぐためです。境界構造体の確保に失敗した場合は即座にアボートします。
POSIX ネイティブの通常オブジェクト、`task.c`、および `trap.c` は、弱いシンボル（weak）である `tsuzuri_trap_hooks`（`owner`、`item`、`resume`、`allocate`、`release` の 5 つの関数ポインタ）を共有します。`trap.c` のコンストラクタがこのテーブルを設定し、境界ランタイムがリンクされていない場合は全スロットが NULL となります。公開 API の `tsuzuri_alloc`／`free` もこのテーブルを経由するため、通常オブジェクトと境界付きオブジェクトのどちらを先にリンクしてもメモリ確保と解放の実装が完全に一致し、ホスト側の外部関数が返した所有バッファも境界の追跡機構へ安全に登録できます。未定義の弱い関数への参照は行いません（macOS のリンカーがこれを拒否するためです）。
`task.c` は、フックテーブルの `owner` が存在する場合、タスクグループを投入したスレッドの境界情報を取得し、各ワーカー（および投入スレッド自身）のタスクアイテムを `tsuzuri_boundary_item` の保護下で実行します。アイテム内でトラップが発生した場合はグループ内の最小インデックスとして記録され、以降のアイテム開始は中断されます。全アイテムの終了後、投入スレッドが `tsuzuri_boundary_resume` を通じて境界へ復帰します。トラップが発生したタスクの内部状態は未定義となるため、先行して `Err` を返したアイテムが存在する場合でもトラップが優先して返されます。
C ランタイム（`task.c`、`cpu.c`、`io.c`）は動的メモリ確保を行わず、メモリ確保の経路は `@tz.alloc`／`@tz.realloc`／`@tz.free` のみに限定されています。`os.c` も結果として返す所有バッファのみを `tsuzuri_alloc` で割り当て、パス名解決・ディレクトリ列挙・出力取得のための一時バッファおよび内部ファイルテーブルには標準 libc の `malloc`／`realloc`／`free` を直接使用します（ファイルテーブルはオープン中のファイルが存在する間のみ維持され、一時バッファは関数呼び出し内で即座に解放されます）。これにより境界ランタイムの追跡オーバヘッドを回避しています。また、`arguments.c` は `main` に渡す引数配列および各文字列のみを `tsuzuri_alloc` で確保します（WASI では `args_get` の一時領域も `tsuzuri_alloc` で確保・解放します）。Windows COFF におけるランタイム埋め込みオブジェクトの結合は `E2002`、extern コールバック（E12）との併用は `E2000` エラーとなります。
検証は、C 言語テスト `tests/trap_boundary_runtime.c`（ASan・UBSan・TSan、並列度 1〜32）、`tests/trap_return.mjs`（ネイティブオブジェクトおよび IR、`-O0`／`-O3`、残存確保数 0 の確認）、ならびに `src/main.rs` と `src/driver.rs` の単体テストによって担保されています。

ネイティブ実行ファイルにおけるスタック枯渇の検知（E14 Phase 3）は、`src/runtime/stack.c` が担当します。コンストラクタがメインスレッドに `sigaltstack` と SIGSEGV／SIGBUS ハンドラを登録し、`task.c` のワーカーは `-DTZ_STACK_GUARD` 定義時に `tsuzuri_stack_thread()` を呼び出して自身のスタック領域を登録します。
フォールト（障害）アドレスが登録スタックの下端監視領域（ガードウィンドウ）に含まれる場合は、標準エラーに `trap: stack overflow` を出力して `abort()` します。監視領域外のフォールトであれば、OS のデフォルト動作へ戻してシグナルを再送出します（別スレッドで同時にスタック溢れが発生した場合でもエラー報告が欠落することはありません）。スタック監視ウィンドウの境界は、macOS では `pthread_get_stackaddr_np`、Linux メインスレッドでは `getrlimit(RLIMIT_STACK)` と `AT_EXECFN` の末尾情報（musl の `pthread_getattr_np` がメインスレッドで現在のマッピング幅しか返さない制限への対策）、ワーカースレッドでは `pthread_getattr_np` を用いて決定します。
このスタック保護ランタイムはネイティブ実行ファイルのビルド時のみ、かつ `llvm::has_recursion` によって関数呼び出しグラフに閉路（再帰呼び出し）が検出されたプログラムにのみリンクされます（再帰しないプログラムではスタックオーバーフローが発生し得ず、ビルド時間を余計に 20〜60 ms 増加させないためです）。デバッグ情報が無効（`-g` なし）の場合は同一の Clang 呼び出し内で `-x c` としてコンパイルし、`-g` 有効時は DWARF を持たない別オブジェクトとして分割コンパイルします。
`driver::run` は子プロセスの標準エラー出力を監視し、`trap: stack overflow` を検知した場合は `E2005`（stack overflow）として明示的に報告します。この文字列が見つからない場合は従来の `probable_stack_exhaustion` によるシグナル推定へフォールバックします。なお、`tsuzuri test` のランナーは失敗したテストの標準エラーの末尾を表示しますが、テストの実行ファイルは `stack.c` を持たないため、この詳細報告は行われません。
動作は `tests/stack_overflow.mjs`（macOS、`-O0`／`-O3`、メインスレッド、ワーカー、`-g`、非再帰プログラムでのハンドラ非含有、スタック外フォールトの区別）、`tests/trap_locations.rs` の `run_reports_stack_overflow`、および `llvm.rs` の再帰検出テスト群によって検証されています。

ネイティブ環境における CPU ディスパッチ（CPU dispatch）は、`emit_native_build` の経路（native の exe／object と、object を作る `--trap-mode return`）でのみ有効化されます。同梱 kernel は、同梱の Array ソースが確認された場合に限り、単相化された整数 8 型の `Array.sum`・`Array.min`・`Array.max` の本体を `llvm::CPU_KERNELS` の `tsuzuri_cpu_{sum|min|max}_{型名}` 呼び出しに置き換えます（`min`・`max` は非公開 helper `min_index`・`max_index` の本体を置き換えます）。
通常の LLVM API 呼び出しや `--emit llvm` では従来の独立した IR 構造を維持し、ドライバーが実行ファイルやオブジェクトを生成する際、IR が `@tsuzuri_cpu_` で始まる関数を宣言した場合にのみ `cpu.c` をタスクランタイムと同一の C 結合経路へ追加します。
`cpu.c` は level（0 baseline、1 SSE4.2、2 AVX2、3 AVX-512、4 SVE、5 SVE2）を一つの `_Atomic int` に初回だけ決め（`tz_cpu_level`、acq-rel cmpxchg）、kernel の入口は `TZ_CPU_PICK` の switch で level の clone を呼びます。機能フラグは bit 0 が SSE4.2、bit 1 が AVX2、bit 2 が AVX-512（F・BW・CD・DQ・VL と XCR0 の opmask・ZMM 状態）、bit 16／17 が Linux AArch64 の `getauxval` による SVE／SVE2 です。AVX2 の選択には OSXSAVE、AVX、および XCR0 による XMM/YMM レジスタ状態の保存サポートを必須条件とします。kernel の自動選択は AVX2 までで、AVX-512 と SVE は `TSUZURI_CPU_FORCE` の指定時だけ使います（F08 D9）。
kernel の本体は Clang のベクトル拡張と `__builtin_reduce_*`・`__builtin_elementwise_*` で書いた macro を level ごとの target 属性で展開した clone です。和は符号なしの折り返し加算、最小・最大は値を求めてから 2 回目の走査で最初の位置を求めるので、どの clone も同一の結果を返します。GNU ifunc や compiler-rt の CPU モデルには依存しません。
`@cpu` を付けた利用者関数（F08 Phase 3）は、`FunctionEmitter::emit` が define 行に `"tz-cpu"="<levels>:<関数 id>"` を付け、`src/llvm_cpu.rs` の `multiversion` が trap 計装後の IR を書き換えます。元の関数は `.cpu.baseline` に改名して debug 情報を保ち、level ごとの `.cpu.<名前>` 版は `"target-features"` を付けて debug 情報を外します。元の名前には、関数ごとの `@"<名前>.cpu"` に `tsuzuri_cpu_pick(levels)` の結果を monotonic で cache し、switch から各版を tail call する stub を置きます。stub は元の subprogram を複製した自分の subprogram と呼び出し位置を持つので、portable 版が stub へ inline されても inline 位置が保たれます。版ごとに 256-bit ベクトルを渡すレジスタが異なるため、版の本体から 256-bit ベクトルを含む型で直接呼ぶ関数は推移的に同じ level の版を作り、関数値や extern へ渡す呼び出しは `E1005` にします（型検査も `validate_cpu_functions` で同じ規則を先に検査します）。選べる level は `cpu::host_levels`（x86-64 の Windows 以外、AArch64 Linux）で、それ以外の build 先では属性を外すだけです。
公開ランタイムのエントリーシンボルは weak かつ hidden 属性となっています。

256-bit の SIMD 型（F08 Phase 2）は LLVM では 32 バイト境界ですが、heap・配列要素・frame・union の payload は 16 バイト境界までしか揃えないため、`llvm_simd::with_vector_alignment` が `emit_program` の最後に 256-bit ベクトルを含む型（名前付き型は推移的に判定）の load／store へ `align 16` を明示します。256-bit ベクトルを含まない IR は変わりません。

外部関数インターフェイス（`extern`）は `Program.externs` にシグネチャを持ち、通常関数呼び出しと同様に型付きの `HostCall` ラッパーへ lowering されます。外部 ABI は型検査時に具体的な型として確定され、通常の関数値、部分適用、所有権モデル、および呼び出し特殊化の最適化機構をそのまま共有します。
`HostCall` は副作用を持つ式として登録され、extern ラッパー関数自体はデッドコード削除のルート（到達性ルート）から除外されます。
宣言は `BTreeSet` により重複なく 1 回だけ生成され、WASM 属性によってモジュール名と関数名が固定されます。レコードの正規化、バッファ型、ポインタおよび UTF 検証、アロケータの機構は既存のものを再利用します。
所有権付きバッファを戻り値として受け取る場合は、出力記述子の長さを事前に `-1` で初期化しておき、未設定、負数、オーバーフロー、または不正なポインタ範囲を受領時に厳密に検証・拒否したうえで、通常の `drop` 処理へと引き渡します。

`extern` 宣言には明示的なリンクシンボル名（`extern "symbol" def`、`extern "module" "symbol" def`）を指定できます。`HostImport.explicit` が真である場合、指定されたシンボル名がそのままネイティブのリンク名および WASM インポート名となり、モジュール名は `wasm_module`（デフォルトは `tsuzuri`）に設定されます。指定のない extern の関数名、IR、C ヘッダー、および WASM インポートの形式は従来と変わりません。
シンボル名には 255 バイト以下の有効な C 識別子を指定する必要があり、`tz_`、`tsuzuri`、`__` で始まるプレフィックス、および `RESERVED_HOST_SYMBOLS`（生成 IR や数値ランタイムが内部で使用する予約シンボル群）は拒否されます。同一シンボルに対する複数の extern 宣言は、関数型と WASM モジュール名が一致していれば単一の `declare` に集約されます。明示的なシンボル名を持つ extern のプロトタイプ宣言はホスト側のヘッダーが保持することを想定しているため、コンパイラが生成する C ヘッダーには出力されません。

外部ハンドル型 `extern type Name` は、修飾名を持つ `Type::Handle` として表現され、Copy も Clone もできず、デストラクタ（drop glue）も持たない純粋なリーフ（末端）型です。スコープ終了時にリソースを自動解放したい場合は、`Drop` を実装した record 型でラップします。LLVM 上では汎用ポインタ `ptr`（wasm32 では `i32`）として表現され、`ref H` はスロットから読み出したハンドル値そのものをホストへ渡します。
ハンドル型を配置できるのは、extern 関数、export 関数、およびコールバック関数の引数と戻り値の位置に限定され、record のフィールドやバッファの要素型として直接埋め込むことはできません。生成される C ヘッダーには、プロトタイプ宣言に先立って `typedef struct tz_handle_<長さ付き修飾名>_s *` が 1 度だけ出力されます。

ネイティブ実行ファイル向けのリポジトリ外ライブラリのリンク入力は、CLI の `--link`、`-l`、`-L`、およびルートパッケージのマニフェスト内 `[native]` セクションで指定します。Clang の呼び出し時には、既存引数の後、かつマニフェストでの指定、CLI での指定の順序で `-L`、ライブラリパス、`-l` が追加されます。
ビルドキャッシュのキーは外部リンク入力のファイル内容を追跡できないため、リンク入力が指定されている間はビルドキャッシュの使用を安全のためにバイパスします。出力先パスが入力ファイルと同一である場合は `E2003`、ネイティブ実行ファイル以外のターゲット、または `check`／`fmt`／`doc` コマンドでリンクオプションが指定された場合は `E2000` エラーを報告します。

コールバック関数は、関数型の引数を取る外部関数（callback extern）をすべての引数を指定して直接呼び出し、その関数引数の位置にトップレベルで定義されたユーザー関数の名前のみを渡す場合に限定して許可されます。`check.rs` の `validate_callbacks` が型検査後にコールグラフを走査して構文を検証し、許可されないパターンは `E1008` で拒否します。
呼び出し側は `host_call` を直接発行し、callback extern 自体の関数本体は生成せず、クロージャ記述子（`%tz.closure`）も作成しません。関数引数の実引数としては `ptr @tz.callback.<修飾名>` が渡され、`llvm_abi::wrapper` が内部用の C ABI ラッパー関数を対象関数ごとに 1 つずつ生成します。
wasm32 ではラッパー関数のアドレスが関数テーブルのインデックスとなるため、IR 内に `@tz.callback.` が存在する場合にのみ `wasm-ld` へ `--export-table` を渡します。コールバックを使用しないプログラムの出力成果物に変更は生じません。

## 不変条件

**Whole-build cache:** `cache.rs` はストリーミング形式の SHA-256 実装と NIST テストベクトルを備え、既存の `serde_json` を用いてメタデータをシリアライズします。型検査と IR 生成の途中段階のキャッシュは作成しません（パースの結果は次の frontend cache が持ちます）。
コンパイラ実行バイナリ全体のダイジェストハッシュをキャッシュキーに含めることで、同一の Git コミット上に存在する未コミットの開発版バイナリ同士も厳密に識別します。`build.rs` が埋め込む Git コミット文字列のみに依存することはありません。
キーは長さ付きフィールドで構成され、コンパイラバージョン、ホスト環境、ビルドオプション、実行アクション、すべてのソースファイルおよびマニフェストのパス・内容バイト列・由来（origin）、生成された IR、コンパイラおよび依存ツールのバイナリダイジェスト、ツールの `--version` 出力、および関連環境変数を含みます。
`--cpu native` 指定時には、Clang が定義するターゲットマクロ群もキーに含めます。ファイルパスは Unicode 正規化を行わず OS の表現をそのまま維持し、絶対パスによるソースマップとパッケージの識別性も分離して管理します。
アーティファクト、トラップ情報、DWARF デバッグ情報はステージング領域から復元され、実行権限およびサイドカーファイルを完全に維持した状態で公開処理（`publish_outputs`）へ渡されます。キャッシュのヒット時・ミス時のいずれにおいても、ソースファイル、マニフェスト、およびハードリンクの保護チェックを迂回することはありません。
キャッシュルートディレクトリには専用のマーカーファイルが必須であり、シンボリックリンクは拒否されます。各キャッシュファイルのサイズと SHA-256 ハッシュが検証され、メタデータの欠落、ファイルの破損、または未知のフォーマットを検出した場合は安全にキャッシュミスとして扱います。信頼境界は同一 OS ユーザーのプライベートキャッシュとして定義されます。
保存処理ではキーごとに非待機の排他ロック（create-new lock）を獲得して複数プロセスの重複書き込みを防止し、全ファイルの出力完了後にディレクトリのリネームによってアトミックに配置します。ロック競合が発生した場合はキャッシュ保存をスキップし、ビルド処理をブロックさせません。I/O 障害は警告として報告し、生成されたビルド成果物自体はそのまま保持します。
GC（ガベージコレクション）は最大 4096 エントリまで走査し、最終アクセス日時、合計 2 GiB の容量上限、30 日間の有効期限を基準に最大 128 件ずつ回収します。古い不完全エントリ、残存ロック、一時領域も回収対象であり、上限はソフトリミットです。
macOS のデバッグ実行ファイルとデバッグ共有ライブラリにおける DWARF は出力ファイル名に依存した情報を含むため、この場合に限って出力先パスもキャッシュキーに含めます。Windows のデバッグ実行ファイルは隣の PDB をファイル名で指すので、その名前もキーに含めます（G16）。`--emit shared` はファイル名を install name（macOS）か soname（Linux）として埋め込むので、出力のファイル名もキーに含めます（`hash_output_path`）。それ以外の場合における別出力先へのアーティファクト再利用性は維持されます。
動作は `tests/cache.mjs` により、実際の CLI を用いたキャッシュヒット（ツール起動回数の削減確認）、ミス、破損時の回復、並行書き込み、no-cache 指定、実行権限の保持、トラップ情報／DWARF の整合性、依存関係変更時の無効化が検証されています。

**Frontend cache（G17）:** `check`・`build`・`run`・`script`・`test`・`bench`・`doc` は、構文解析の結果をキャッシュルートの `frontend/` に保存して再利用します（`Project::analyze_cached` から `analyze_inputs_with`）。`src/syntax_codec.rs` は `Program` から届くすべての構文型を手書きの `Wire` で符号化します。enum の tag は宣言順の 1 byte、整数は LEB128、文字列と列は長さ付きで、`match` は `_ =>` を使わず全 variant を書くので、構文型に variant や field を足すとここが compile error になります。span の source index は保存せず、復号のときに現在の index を付け直すので、ファイルを足したり消したりして index がずれても再利用できます。
プロジェクトのルート（`fs::canonicalize` した場所）と解析の種類（`program`・`tests`・`docs`）ごとに、パック `p-<project key>.tzp` と manifest `m-<project key>.json` の 2 ファイルだけを読み書きします。パックは、ソースの byte 列と compiler の同一性から作る parse key ごとの entry（header、`syntax_codec` の符号、SHA-256）を並べたものです。ソースごとのファイルにしないのは、ファイルを開く費用（on-access scan のある環境では 1 回 0.3〜1 ms）がモジュールの構文解析より大きくなるためです。manifest はモジュールごとの parse key と interface hash を持ち、前回との比較から `FrontendDelta`（変わったソース、変わった interface、消えたモジュール）を作ります。interface hash は span・文書 comment・関数本体・test・入口式・instance method の本体を除いた符号（`Mode::Interface`）の SHA-256 です。compiler の同一性は形式番号、版、実行ファイルの大きさと更新時刻に、Unix では device・inode・状態変更時刻（ctime）、それ以外ではパスと作成時刻を加えたものです（`cp -p` や Nix のように更新時刻をそろえても、別のファイルや書き直したファイルを区別します）。
パックと manifest は、すべてのソースの構文解析が成功したときだけ、内容が変わった場合に書き換えます。読めない root、symlink、大きさの超過、形式・compiler・プロジェクトの不一致、checksum の不一致、復号の失敗はすべて miss として構文解析し直し、診断を出しません。構文解析の順序、`Diagnostics::is_full` による打ち切り、エラーの連結は cache なしと同じなので、IR、診断、警告、終了コードは cache の有無や cold／warm で byte 単位で同じです。書き込みは一時名からの rename で、書いたプロセスが合計 1 GiB、更新から 30 日、1 時間より古い一時ファイルの規則で掃除します（1 回に 4,096 件を調べ、最大 128 件）。`BuildCache::evict` は `frontend` という名前を扱いません。言語サーバー（`analyze_inputs_semantic`）は cache を使いません。`TSUZURI_CACHE_DIR=`（空）で無効になり、build・run・script の `--no-cache` は両方の cache を、repl の `--no-cache` は成果物の cache を止めます（repl は frontend cache を使いません）。検証は `src/syntax_codec.rs`・`src/frontend_cache.rs` の単体テストと `tests/frontend_cache.mjs` です。

**bindgen:** `tsuzuri bindgen`（`src/bindgen.rs`）は lexer・parser・check・LLVM を通らず、生成したテキストは利用者のソースとして通常の経路で検査されます。生成するのは C の ABI がホスト ABI と一致すると確かめられる宣言だけで、型は Clang の表記（`qualType`）を typedef 展開してから固定の表と完全一致で照合し、表にない表記は推測せず `W2002` で省きます。`desugaredQualType` は typedef とともにその alignment 属性を落とす（`typeof` の先の over-aligned な typedef が素の整数に見える）ので使いません。enum・typedef・struct・フィールドの属性は、配置と呼び出し規約を変えないと分かっているものの許可リスト（`HARMLESS_ATTRIBUTES`）で判定し、それ以外の属性を持つ型は変換しません。C の tag と typedef 名は別の名前空間ですが、Clang は `typedef struct { ... } S;` の型を `struct S` と表記するので、無名の struct・union を名付ける typedef は展開せず、無名の enum の typedef は同じ名前の enum の tag（入れ子の宣言も含めて全ノードから集める）があれば変換しません。
Clang は JSON の位置に `file` を変化時にしか書かず、`serde_json` の `Value` はキー順を保たないので、宣言の所属ファイルは `loc`（spelling → expansion）、`range`（begin → end）、子（`array_filler` → `inner`）の順をコードで固定して全ノードを訪ねて求めます。Clang は `sqrt` などの library builtin を最初の言及で暗黙に宣言するため、`previousDecl` が暗黙の宣言を指す関数は最初の宣言として扱います。enum 定数の値は `ConstantExpr` の値に `ImplicitCastExpr` の整数変換を適用して求めます。
マクロは AST に現れないので、4・5 回目の Clang の起動 `-E -dD` と `-E -dM` の出力を読みます。行マーカー（`# 12 "/path/x.h" 2`。ファイル名の C エスケープを戻す）が次の行のファイルと行番号を与え、それ以降の各行は次のソース行なので、各 `#define` をファイルと行に結び付け、`#undef` と再定義を反映した最終状態のうちヘッダーのものだけを、ヘッダーのバイト列の行頭表からの位置で宣言と同じ順に並べます。`-E -dD` は `#pragma push_macro`／`pop_macro` を出力しないので、`-E -dM` の最終的なマクロ表と置換列が一致しない定義は生成しません（整数リテラルに見えれば `W2002`）。整数リテラルの型は C17 6.4.4.1 を LP64 に当てはめて決め、単項マイナスはその型で計算します。
不透明なハンドルは、ヘッダーが最初に宣言し、翻訳単位のどこでも（入れ子の宣言も含めて）定義されない struct だけです。定義のある struct は呼び出し側が確保しうるので、ハンドルにしません。`--buffer`・`--consume` の注釈は引数の名前か位置で解決し、ヘッダーにない関数・引数や重なりは `E2000`、型が buffer の ABI（要素への const ポインターと直後の 64-bit の要素数）やハンドルに合わない組は `W2002` です。
出力は AST、ヘッダーのバイト列、Clang の版とターゲットだけの関数で、時刻・絶対パス・環境変数を含まず、匿名の tag の表記（`(unnamed struct at /path:1:2)`）からも絶対パスを取り除きます。コメントへ入る文字列は 0x20–0x7E 以外を `?` に置き換え、生成コードへの行の注入を防ぎます。検証は `tests/bindgen.rs`（Clang を起動しない AST 単位のテスト）と `tests/bindgen.mjs`（golden、決定性、`check`／`fmt --check`、C ライブラリとリンクする native の `-O0`／`-O3` の往復、CLI と出力保護）です。

**Windows MSVC:** `native_compile_args` はコンパイラが対象とする CPU アーキテクチャに応じて `x86_64-pc-windows-msvc` または `aarch64-pc-windows-msvc` を選択し、POSIX 向けフラグと明確に分離します。Win32 タスクアダプタは、既存のスケジューラに対して SRWLOCK、CONDITION_VARIABLE、INIT_ONCE、CreateThread、WaitForSingleObject、CloseHandle による同期・スレッド機能を提供します。
`windows_abi` は型検査済みのエクスポート関数一覧にのみ `dllexport` を付与し、標準出力への書き込みは MSVCRT の `_write` による 32-bit カウントおよび戻り値から安全にサイズ拡張して扱います。UTF-8 のバイト列を損なわないよう標準出力のファイル記述子をバイナリモードに設定し、システムのコードページは改変しません。
Windows 環境にも 128-bit 整数のヘルパー関数群が同梱されており、MSVC CRT のみで安全にリンク可能です。CPU ディスパッチは Windows 上では移植性の高いベースライン実装に固定されます。
Rust 内部でのファイル同一性判定には Windows 専用の安全な同一ファイル比較 API を使用し、unsafe の禁止ポリシーを維持します。`fs::rename` による既存出力の安全な置換とハードリンク保護は Windows 専用テストで検証されています。
COFF におけるランタイム埋め込みオブジェクトの結合は `E2002` エラーとなります。実行ファイルの生成を優先し、LLVM IR とランタイムを一度だけ明示的にリンクする経路を維持します。
Windows CI および `tests/windows.mjs` を配備。macOS 上での実 Windows SDK による C および IR の `-O0`／`-O3` COFF/PE リンク、ならびに全 Rust ターゲットでの Windows 向け cfg の成立を確認済みです。

**結果付き Task:** `Task.parallel_results` は通常の組み込み関数の枠組みとコールドなタスククロージャを経由し、専用の `TypedExprKind` として `llvm_task.rs` へ下げられます。
ランタイム ABI は `i64 tsuzuri_task_parallel_results(i32 (*run)(context, index), context, length)` で定義され、戻り値 `-1` は全タスクの成功、それ以外の非負値は最初に失敗したタスクの最小インデックスを表します。
ネイティブおよびスレッド対応 WASM の既存タスクグループ構造体に結果実行関数と失敗追跡フィールドを追加し、キューのロック内で未配布の長さを短縮して残存カウントから未開始分を減算します。開始済みタスクの残存数が 0 になるまで呼び出しスレッドは待機を継続します。
LLVM は一時的な `Result` 配列と、タスク開始フラグを記録する `i8` の `started` 配列をスタック上に確保します。コールバック関数は環境を消費して実行結果を書き込んだ後に `started` フラグを公開し、ジョイン（合流）後はそのフラグを用いて「初期化済みの結果」と「未開始のクロージャ」を厳密に区別します。
成功時のペイロードは出力配列へ移動され、エラー時は最小インデックスのエラー値のみが戻り値へ移動され、その他の値は既存の `drop` 処理によって確実に解放されます。一時 `Result` 配列全体に対するデストラクタ呼び出しは行わず、生のバッファのみを解放するため、移動済みのスロットが二重解放されることはありません。
再帰的な `Result` 型の外側ノードも、ペイロードの移動完了後に解放されます。トラップ情報（trap-info）は既存のコールバック ABI を維持し、タスクの実行コードに対してのみコンテキストを伝播させます。
テストは `tests/tasks.mjs` および `wasm_threads.mjs` により、41 件の実行結果、4 件のトラップ、所有配列・クロージャ・再帰ペイロードの取り扱い、結果の順序性、最小エラーインデックスの選択、およびコールド破棄が検証されています。C 言語のスケジューラは ASan／UBSan／TSan を適用した環境でも安全性が確認されています。

**HKT（高階型）:** `ClassDecl.kind` は明示的なカインド情報を保持し、`TypeExprKind::Apply` のヘッド識別子が `'f` で始まる場合は型変数への適用として扱われます。既存の名前付きヘッドおよびフォーマッタの経路はそのまま維持されます。
クラスのコンストラクタのアリティ（引数の数）とシグネチャ・制約から導出されるカインド環境を参照し、値型の出現位置には型の完全な飽和（全引数の適用）を要求します。初版におけるカインド引数はすべて通常の `Type`（`*`）に限定され、高階カインド引数や型別名の引数は拒否されます。
`Type::Partial` はコンストラクタ宣言と末尾固定引数を保持し、`Application` はヘッドと適用引数列を保持します。`Type` 構造体全体のサイズは 4 ワードに抑えられており、`map_type`、`substitute`、および `Inference::resolve` の各処理において型が飽和した段階で、既存の Record、Union、Array、List、Vec、Task などの具象型へ正規化されます。
HKT メソッドの解決では、クラスのレシーバ型と固有の値型変数をフレッシュな型変数として生成し、具象インスタンスの完全なシグネチャからメソッドの特殊化引数を決定します。デフォルトメソッドの実装も、既存の内部関数および共有の `Specializer` を利用して展開されます。
インスタンス宣言のヘッドとメソッド定義で同名の型変数が使われている場合は、生成されるメソッド関数の型変数をアルファ変換によって分離し、名前の意図しない捕捉を防止します。
値の型表現に `Partial` や `Application` が残存したまま LLVM コード生成へ到達した場合は、入口の整合性検査で `E1015` エラーとなります。名前付きジェネリクス宣言における通常の型変数（`Type::Variable`）は従来どおり維持されます。
検証は `tests/higher_kinds.rs` および `tests/features.mjs` の `higher_kinds` スイートにより、Maybe、Result、デフォルトメソッド、ジェネリック関数、ローカル型注釈、すべてのコンストラクタ形状、および所有ヒープ領域の挙動が網羅的にテストされています。標準ライブラリに `Functor` などの型クラスを暗黙的に自動導入することはありません。

**GPU Phase 1:** `gpu.rs` は、通常の型検査および単相化が完了した型付き IR を走査し、既知の関数呼び出しからなる有向グラフを抽出します。コンパイラ本体とは独立した型推論器や数値評価器を新設することはありません。
`GpuKernel.cpu_reference` は既存の LLVM emitter を再利用して生成されます。`Gpu.init` および `Gpu.map` のコールバックに対する各種制限はクロージャの lowering 後に検査され、ファーストクラスの関数 API を経由した制限の回避は確実に拒否されます。
`std/Gpu.tz` は明示的な `CpuReference` のみを構築します。不透明な `Device` および `Buffer` が非 Copy（所有型）である性質は、型システムとシンボリック所有権検査の両面で統一的に扱われ、内部フィールドへのアクセスは既存の不透明型保護ゲートによって遮断されます。
WGSL コード生成では、各部分式が順序付きの一時 `let` 変数へと評価され、`if` や短絡論理演算の分岐内に存在する式はその分岐の内部でのみ評価されます。同一ビット幅の整数キャストには `bitcast` を用い、符号付き算術は `u32` のラッピング演算を経由して表現します。
WGSL の型制約および浮動小数点・トラップの言語契約に基づき、シェーダー内での `i64`、`f64`、厳密な浮動小数点演算、除算、剰余算の使用は拒否されます。CPU 参照実装での許容とシェーダーコード生成での許容を混同することはありません。
CLI の `--emit wgsl` オプションは、単一のエクスポート関数を持つカーネルプロジェクトを通常の安全な出力保護経路を通じて公開します。WebGPU ホストの試作実装は、デバイスの各種制限（limits）、シェーダー診断情報、所有バッファ、エラースコープ、およびコマンドキューの同期を処理し、GPU 実行の明示要求を勝手に CPU 実行へ縮退させることはありません。
LLVM において列挙型のスカラー別名は前方参照できないため、すべての enum 別名は record／union 構造体の定義よりも前に先行して生成されます。構造体同士の相互前方参照は従来どおりサポートされます。
検証は `cargo test --test gpu` および `tests/gpu.mjs` で行われます。`TSUZURI_WEBGPU=1` 環境変数設定時には、実際の GPU アダプタ上でシェーダー実行、初期化／マッピング、常駐チェーン、および境界の動作がテストされます。

**モジュール:** 「1 ファイルにつき 1 モジュール」を強制し、モジュール名はファイル名から自動的に導出されます。
プロジェクトルート配下を再帰的に走査し、`SourceFile.relative_path` を正規化した順序で処理します。例えば `Geometry/Point.tz` のコンパイラ内部名（キー）は `Geometry.Point` となり、ソースコード上では `Geometry::Point` と記述します。
ファイル単体を指定した入力ではその親ディレクトリがルートとなり、ディレクトリ入力を指定した場合はそのディレクトリ自身がルートとなります。隠しファイル／ディレクトリ（ドット始まり）は無視され、ソースファイルやディレクトリに対するシンボリックリンクは拒否されます。
ソースファイル数は最大 4096 件、ディレクトリ数は最大 1024 件、モジュールパスは最大 16 階層／255 バイトを上限とし、標準ライブラリの予約名はパスの先頭要素に対して適用されます。
拡張子ごとに明確な役割があり、`.tz` は通常のコード、`.tt` は複数の型クラス宣言、`.tc` は単一のコンピュテーション式ビルダー実装に限定されます。
型クラスの宣言は `.tt` のみに制限され、そのインスタンス実装は `.tz` または `.tc` 内で行われます。
同一ディレクトリ内で拡張子のみが異なる重複ファイル名は拒否されます。旧拡張子 `.tzr` は入力を拒否し、自動列挙の対象外です。
`analyze_modules` は `("Name.tz", source)` などのペアを受け取り、ファイル種別を含めて厳密に検査します。一方、互換用の拡張子なし `("Name", source)` や `analyze(source)` はファイル種別未指定のインメモリ AST を検査し、既存のフロントエンド利用者の利便性のために混在宣言を許容します（ドライバーは必ず拡張子を渡すため、この互換経路は使用しません）。
関数やレコードはモジュール名で修飾された一意な完全名を持ち、LLVM の内部シンボルにもこの修飾名が用いられます。
他モジュールの関数を呼び出す際は修飾が必須ですが、レコード型、union、case、型クラスについては自モジュール優先で解決し、他モジュールで一意に定まる場合は無修飾での解決を許可します（標準ライブラリとの優先順位は後述）。型付き IR 上の関数およびレコード参照は、プロジェクト全体で一意な整数 ID として管理されます。
公開 ABI は従来の `tz_name` を維持し、エクスポート関数名の衝突は型検査時に拒否されます。
`private` による可視性制御は型検査時の名前解決（`Names` の `NameInfo` による可視性判定）でのみ完結し、型付き IR、LLVM の内部シンボル、公開 ABI の形状を変更しません。他モジュールの private な要素は、無修飾レコード名の解決候補にも曖昧性エラーの候補にも含まれません。public 宣言から private 型が漏洩しているかどうかの検査は、解決済みの `Type` ではなく元の `TypeExpr` を走査して行い、漏れた型参照の記述位置で正確にエラーを報告します。

**名前空間:** パーサーは、ファイル先頭の文脈キーワード `namespace A::B` およびそれに続く `using A::B` を読み取り、それぞれ `Program.namespace` と `Program.usings` に格納します。
ソースコード上での名前空間パスの区切りには `::` を用い、コンパイラ内部の名前空間、キー、完全修飾名では `.` を用います。この表記変換は `module_identity`、`module_path`、`namespace_path`、`canonical`、およびマニフェスト境界でのみ行われ、診断メッセージや LSP での表示時には内部の `.` を再び `::` に戻してユーザーに提示します。
字句解析器の `mark_paths` は、空白を含まない `Ident::Ident` の連鎖のうち、末尾の要素が大文字で始まるもの、および行頭の `namespace`／`using` に後続するものを `TokenKind::PathSep` として識別します。`def`／`rec`／`and` の宣言名直後の記号や、パターンマッチ等における `x::xs` は通常の `DoubleColon` のまま維持されます。
パーサーは `PathSep` で結合された要素列を `A::B::Mod` という単一のモジュール識別子として読み取り、モジュールメンバーへのアクセスは従来どおり `.` の連鎖として解析します。
コンパイラがキーから合成する内部修飾名（エイリアス展開、derive、単相化）には先頭に `::`（`check::KEY_PATH`）を付与し、利用者がソース上で記述した名前空間パスと厳密に区別します。
`Type::display` は、record、union、および extern type をキーから生成した正規名で表示し（`check::type_display`）、モジュール名と同一の名前を持つ型についてはモジュール名のみに短縮して表示します（例: `Geometry.Point.Point` は `Geometry::Point`、`Maybe.Maybe<i64>` は `Maybe<i64>`）。内部名、型付き IR、LLVM 上の型名は `Geometry.Point.Point` のまま保持され、型情報から構文を再構成する単相化（`polymorph::key_name`）でもキー名が使用されます。
`SourceFile.namespace` はルートパッケージのデフォルト名前空間（マニフェストの `namespace`、パッケージ名の PascalCase、マニフェスト不在時はディレクトリ名）を保持し、外部依存パッケージおよび標準ライブラリでは空となります。
標準ライブラリ（std）のモジュールは、`lib::analyze_inputs_indexed_all` および LSP の `ModuleNames` によって名前空間 `stdlib::NAMESPACE`（`std`）に配置され、キーは従来どおりファイル名（例: `Maybe`）となります。
`lib::module_identity` は、相対パス、ファイル内宣言、およびデフォルト名前空間からモジュールのキーと名前空間を算出します。キーは、名前空間宣言のないファイルでは従来の相対パス名、デフォルト名前空間の内部を明示宣言したファイルではデフォルト名前空間を除いた名前、それ以外では完全名となります。
キーは型検査、型付き IR、LLVM シンボルの修飾名として機能するため、デフォルト名前空間を明示宣言しても生成される IR は変化しません。エントリーポイントの選定もキーではなく `ModuleInput.entry`（ルートの `Main.tz`）に基づいて行われます。
`check::Names` は完全名からキーへのマッピングテーブル、およびキーごとの名前空間と `using` の解決結果を保持します。`module_path` は `Name` または `A::B::Name` を受け取り、参照元の名前空間、`using`（モジュール名のみ）、外側の名前空間、グローバルスコープの順で完全名を検索し、モジュール名単体の場合は最後にキー名として検索します（`.` を含むパスはモジュールを指しません）。
標準モジュールの完全名は `std.Maybe` であるため、`std::Maybe` はグローバルスコープの検索でヒットし、名前空間を省略した `Maybe` は最後のキー検索によって解決されます。
ユーザー定義モジュールの完全名が `std` で始まっている場合は `check_modules_collect` が `E1011` エラーを報告し、`package::valid_namespace` およびマニフェスト検証でもデフォルト名前空間としての `std` は拒否されます。
組み込み関数解決の `qualified_builtin` は、`std::Task.run` のような修飾から `std::` を除去して組み込み関数を検索し、パーサーは型シグネチャ中の `std::Task` や `std::Vec` を通常の `Task` や `Vec` として解釈します。
`case_path` は、名前空間を指定しない `Maybe.Some` のような参照において、`Maybe` が std モジュールであり参照元スコープに `union Maybe` の定義が存在する場合、参照元の union の case を優先して返します。`docgen` は標準ライブラリのページに `Namespace: std` を明記します。
`canonical` は、最後の `::` の直後から最初の `.` までをモジュール名とみなしてキーへ置き換える正規化を行い、関数、case、型、クラス、アクティブパターン、およびビルダーの検索に先立って 1 回だけ適用されます（`KEY_PATH` が付与された内部名はそのマーカーを除去するのみです）。
名前空間とモジュールを `.` で連結したパスは解決されず、`E1002` または `E1004` の診断メッセージにおいて `namespace_spelling` を通じて `::` を用いた正しい記法を案内します。
モジュールのパスは同名の record、union、型別名、および extern type も表すことができ、その型の完全名はモジュールの完全名と一致します。`check_module_type` は `Sample::Point.Point` のようにモジュール名を重複して重ねた型の記述を、`case_path` は `Sample::Shape.Shape.Rect` のような記述を `E1004` として拒否します（`KEY_PATH` 付きの内部名は対象外です）。診断メッセージやホバー表示では `type_spelling` および `semantic::type_name` によって正規の名前が提示され、LSP のメンバー補完候補からその型は除外されます。`using` の競合による曖昧性は `check_path` および `check_module` が検索の入口で検知して `E1004` とします。LSP は `ModuleNames` を通じてこれらと同一の規則を再現し、コード補完（`::` の入力後は子名前空間、`.` の入力後はメンバー）、シグネチャヘルプ、およびセマンティックトークンの生成に活用します。

**パッケージ:** パッケージ読み込み層は、`Tsuzuri.toml` で定義されたローカルパス依存、commit 固定の git 依存、registry の版の要求を取り扱います。`package.rs` が限定的なマニフェスト文法、`Tsuzuri.lock` の読み書き（`parse_lock`／`render_lock`）、内容ハッシュ（`content_sha256`）を受け持ち、ドライバーの `load_packages` は明示的なスタックを用いて依存グラフの循環参照、パッケージ名と取得元の一意性、およびリソース上限を検査します。
`SourceFile.package` には正規化されたルートパスとパッケージ名からなる `PackageId` が格納され、依存パッケージの名前空間が各ファイルの `relative_path` に付与されます。型検査における User／Std の分類や `private` 可視性の境界判定自体は変更されません。
マニフェスト情報は `Project.manifests` に保持され、`source_for` においては通常のソースファイルの後に続く ID として参照されます。ビルド成果物およびドキュメントの出力保護機構は、ルートパッケージと依存パッケージの両集合を対象として適用されます。
すべてのソースファイルは論理パス順に整列され、標準ライブラリ（std）は常に末尾に配置されます。依存パッケージのルートは親プロジェクトのファイル探索スコープから除外され、同一ルートのパッケージが重複して読み込まれることはありません。ビルドスクリプトは実行しません。
git・registry 依存の解決は `load_packages` に渡すリゾルバーだけが異なり、グラフ走査と E04 の規則は取得とビルドで共有されます。`git` を起動しネットワークに触れるのは `tsuzuri fetch` と `tsuzuri publish`（`fetch.rs`）だけです。fetch は走査を 2 回行います。1 回目は git 依存を取得し、版の要求を集めて走査から外し（リゾルバーが `None` を返す）、index の取得と最小版選択（要求をたどる BFS、名前ごとの最大値、互換範囲の検査、選んだ版からの到達可能性）の後、選んだパッケージを取得・検証してから、2 回目の走査で全体の規則を確かめます。fetch は空の hooks と設定で隔離した一時 bare リポジトリへ commit を取得し、`ls-tree -r -z -l` と `cat-file --batch`（要求の書き込みと応答の読み出しを別スレッドで行う）でパッケージのファイルだけを読み、パス・モード・大文字小文字・上限を検査してから、ストア（キャッシュルートの `packages/git/<sha256>/`）へ同じファイルシステム内の rename で確定させます。`BuildCache::open` を先に呼んでキャッシュルートの marker を作るため、ビルドキャッシュと共存し、`evict` はストアに触れません。
ほかのコマンドはオフラインのリゾルバーで `Tsuzuri.lock` とストアだけを参照し、`load_from_root` が git パッケージのソースを読んだ後に内容ハッシュを照合します。`Tsuzuri.lock` は `Project.manifests` に加わるため、出力保護とビルドキャッシュのキーにも含まれます。

**標準ライブラリ:** `std/` 配下のソースコードは `stdlib::SOURCES` としてコンパイラバイナリ内に `include_str!` で静的に埋め込まれており、`analyze`、`analyze_modules`、および `Project::load` の各処理において、ユーザーソース群の末尾に追加されます。
パース順序は入力順に従うため、ユーザー側のソース ID や `Project.root` の値は標準ライブラリの有無によって変動しません。
Rust の単体テスト向けには `analyze_modules_with_std` が用意されており、std の内容を任意に差し替えることが可能です（空スライスを渡せば std なしの環境となります）。
各モジュールは `ModuleOrigin`（`User` または `Std`）の属性を持ち、`stdlib::RESERVED_MODULES` に定義された予約名をユーザーモジュールとして定義することはできません。
std の仮想パスは `std/Name.ext` という平坦な形式で管理され、ファイル出力保護の対象からは除外されます。これにより、ユーザーコードの `relative_path` と混同されるのを防いでいます。
無修飾の型名、case、レコード、型クラスの解決においては、まず自モジュール内、次いで完全修飾名（モジュールパスが同一名の型を表す場合を含む）を検索します。その後、参照元がユーザーコードであれば「ユーザー定義モジュール群 → std モジュール群」の順序で各段階ごとに一意な候補を探索し、参照元が std であれば std モジュール群のみを探索します。
std モジュールにおける `export def` の使用は禁止されており、std の関数を外部から呼び出す際はユーザー定義関数と同様にモジュール名による修飾が必須です。
std のソースコードは型検査の対象となりますが、`closures::lower` の処理後に到達可能性解析（reachability analysis）が行われ、不要な関数は最終成果物から間引かれます。
例外は `stdlib::OPT_IN` の opt-in std モジュール（`Arena`、`Regex`、`Unicode`、`Json`、`Cbor`、`Bench`、`Gen`。D-40・D-41）です。ユーザーのモジュールからの無修飾の解決（`Names::choose`）は opt-in std モジュールの宣言を候補にしないので、ユーザーのコードはそれらを修飾した名前でだけ参照します。
そのため `Project::load` 系（言語サーバーの `load_with_overlays` を除く。REPL の `Project::single_main` を含む）と `analyze_modules_all` は、`stdlib::sources_for` が選んだものだけを読み込めます。`sources_for` はユーザーのソースの ASCII 識別子の並び（先頭の数字を除いた部分も含む）を走査し、`OptIn::names`（モジュール名と、他所の型に instance を与える組み込みクラス。`Json` の `Encode`・`Decode`）のどれかが現れたモジュールと、その `uses` の閉包を加えます。
`stdlib::tests::opt_in_modules_are_reached_only_through_their_names` が std のソースを字句解析・構文解析して、常に読み込むモジュールが opt-in モジュールを名指ししないこと、instance の組み込みクラスが `names` にあること、`uses` が正しいことを検査します。
この選択は、opt-in モジュールの名前を書かないプログラムの型検査の時間（空のプログラムの `check` で約 2 倍になっていた）と IR（関数番号のずれ）を、opt-in モジュールの追加前と同じに保ちます。
関数の由来情報は `CheckedFunction.origin`（`FunctionOrigin`）によって一元管理され、自動生成された `$lambda`、`$task`、`$builtin`、`$case`、`$export` などの補助関数は呼び出し元の `module` や `test` を継承し、`parent` フィールドに親関数の ID を保持します。
LLVM コード生成時、ユーザー由来の関数はすべて出力されますが、std 由来の関数については、ユーザーの通常コード（テスト関数を除く）、エクスポート関数、またはエントリーポイントから参照されて到達可能なもののみが出力対象となります（`reachable_functions`）。名前付きレコードおよび union の型定義についても、ユーザー定義の型、および実際に出力される関数のシグネチャや本体から集められたものだけが出力されます。

`std/Maybe.tc` および `std/Result.tc` は、特別なコンパイラマジックを用いず、通常のジェネリック union と関数のみを用いて型、基本操作、およびビルダーを定義しています。
専用の型付き命令や特殊なランタイムは存在せず、union のタグ分岐、ペイロードの move／clone／drop、および既知の継続に対する特殊化の最適化を一般コードと完全に共有しています。
`get` は網羅的な match の失敗節において `unreachable : unit -> 'a` を呼び出します。
空のブロック本体も成功として反復を継続できるよう、`Zero` 操作は unit の成功値（Maybe では `Some ()`、Result では `Ok ()`）を返します。
スカラー型を扱うビルダーの到達可能な実行経路において、不要な動的メモリ確保や間接関数呼び出しが存在しないことが LLVM IR の検査によって保証されています。
なお、汎用的なアダプタ関数の定義自体にはメモリ確保コードが残り得るため、IR 全文における `@tz.alloc` の有無と、実際の実行ホットパスにおけるメモリ確保の有無を混同しないよう注意が必要です。

**組み込み関数:** `Builtin` は `BuiltinScheme`（型変数、制約、std の型、および `UnsignedOf`／`WidenOf` などの型族）によって型情報を表現し、参照時は具体化された型引数を保持する `FunctionRef::Builtin(BuiltinInstance)` のみを使用します。
モジュール修飾名を持つ組み込み関数（`Task.run` など）は、モジュール関数の解決後、型クラスのメソッド解決よりも前に優先して解決されます。
型族（type family）は引数が具体的な整数型に確定した呼び出し位置で解消され、関数の末尾まで型変数のまま未確定で残った場合は `E1015` エラーとなります。
すべての組み込み関数は scheme に定められた引数個数を持つ `$builtin` ラッパーを経由して呼び出され、部分適用（カリー化）は通常の関数値と同様の仕組みで lowering されます。
LLVM 上の定義は具体化の組み合わせごとに 1 回だけ `@tz.builtin.name` に型引数名を `.` で連結したシンボルとして出力され、型名に含まれる特殊記号は `$A`（`->`）、`$L`／`$R`（`[`／`]`）、`$C`（`,`）などの英数字に置換して LLVM 識別子に適合させます。

**エントリー:** `Main.tz` のトップレベル式を統合した非公開の合成関数、または `Main.main` の関数 ID をエントリーポイントとして保持します。両者の併用は許可されず、他モジュールや `Main.tc` 内の `main` がエントリーに選ばれることはありません。
`Main.main` のシグネチャは `unit -> i32` または `[string] -> i32`（`Array<string>`）のいずれかのみが認められており、チェッカーがシグネチャ収集時に `check::entry_signature` で検証し、それ以外のシグネチャは `E2004` エラーとして拒否します。
トップレベルの `let` は通常のローカル変数束縛へと lowering され、モジュールレベルの共有可変状態を導入することはありません。
ライブラリ出力モードでは、コンソール用ラッパーやトップレベルコードの自動実行コードは追加されません。
トップレベル式の評価値は、数値、`bool`、文字、文字列であればそのまま標準出力へ表示され、それ以外の型は `Display` インスタンスが存在する場合に `to_string` を経由して表示され、インスタンスがなければ破棄されます（`entry_result`）。
IO の `let!` や `do!` の後に通常の式が続くトップレベルコード（`$implicit.root.$entry`）、およびビルダーを持たない既知の戻り値型（`def main :: unit -> i32` の本体の `i32` など）の本体は、IO の bind 処理を std の非公開関数 `IO.run` で順次直接実行します（direct style。直接実行が許可されているビルダーは IO のみです）。

**標準入出力:** `std/IO.tc` は不透明な `IO<'a>` 型の内部に通常の `unit -> 'a` クロージャを保持します。`pure`、`bind`、`map`、`Delay`、`Combine`、`For`、`While`、`MergeSources` はすべて通常の Tsuzuri ソースコードとして記述されており、独自のタスク機構、ガベージコレクタ、あるいはエフェクトインタープリタなどを新設することはありません。
`IO.__read_line` および `__write` は、std 由来の IO モジュールからのみ参照可能な内部組み込み関数です。低水準の読み込み処理は `(i32 * [ubyte])` のタプルを返し、書き込み処理はステータスコードを返します。`Maybe` や `Result` への変換および UTF デコードは std 側のコードで処理されます。
LLVM は既存の `host_result_slot` および `read_host_result` を再利用して記述子の初期化と検証を行います。IO 専用の外部呼び出しに純粋性属性（pure/readnone）は付与せず、所有バッファの解放には一般コードと同一のアロケータおよび `drop` 機構を使用します。
`IO<T>` のエントリーポイントでは `tsuzuri_main` が生成され、通常のクロージャ ABI（引数 `unit`、環境ポインタ、`borrow=false`）を通じて 1 回だけ消費実行されます。結果の値 `T` は `FunctionEmitter` の既存の `drop_value` によって適切に解放され、再帰型の解放登録も共有されます（ただし、プロセスの終了コードとなる `IO<i32>` の値だけは解放せずにそのまま返却します）。ネイティブ実行ファイルでは C の `main` からこれが呼び出され、オブジェクトファイルや WASM 出力ではホスト側からの明示的な呼び出しを想定し、結果の自動表示処理は付加しません。
ネイティブの `io.c` は、必要性が生じた場合にのみ既存のタスク／CPU ランタイムと同一の C 結合経路へ追加されます。標準 C の `fgetc` による stdio バッファから 1 行ずつ読み込み、幾何級数的に伸長するバッファを通常のアロケータで管理します。`EINTR` によるシグナル中断時は自動で再試行し、末尾の LF／CRLF は適切に除去します。`fwrite` は部分書き込みをループで進行させ、flush の失敗もステータスとして呼び出し元へ通知します。
WASM では、`tsuzuri_io` の同期的な `read_line`／`write` およびメモリ／アロケータを必要時のみエクスポートし、ホスト不在を単なる no-op（無処理）として黙認することはありません。デフォルトの計算専用モジュールに対して不要なインポートを増やすこともありません。
ドライバーの `run` サブコマンドは標準入力・標準出力を子プロセスへ透過的に継承し、標準エラー出力はバッファリングしながらリアルタイムで転送します。JSON 診断モードの場合にのみ標準エラー出力を保持し、プロセス異常終了時の診断情報に含めます。
検証は `tests/io.mjs` により、ネイティブおよび WASM の `-O0`／`-O3`、コールド実行、順序性、EOF、文字コード、障害発生時の動作、ABI 境界、対話的 CLI 実行、オブジェクト出力、ASan／UBSan、ならびにメモリリーク（未解放バイト 0）の検証が行われています。

**OS API（E08）:** `File`、`Dir`、`Path`、`Env`、`Time`、`Random`、`Os`、`Process` は通常の std ソース（`std/*.tz`）として実装されており、OS に作用する操作はすべて遅延評価される `IO<Result<_, Os.Error>>` として表現されます（`Path` および `Random.Pcg` は純粋計算です）。コンパイラ本体が提供する低水準境界は、`Os.__read`、`__args`、`__write`、`__random`、`__clock`、`__sleep`、`__open`、`__handle`、`__close`、`__spawn` の 10 個の `Builtin` のみに限定され、新しい型、型クラス、あるいはランタイム関数をコンパイラコアへ追加することはありません。
これらは `IO.__read_line` や `__write` と同様に、std の `File`、`Dir`、`Env`、`Time`、`Random`、`Process`、`Os` 以外のモジュールから直接参照することはできず、ユーザーコードからの参照は `E1022` エラーとなります（`polymorph.rs`）。
第 1 引数に渡される整数によって具体的な操作が選択されます。`__read` は 0: ファイル読み込み、1: ディレクトリ一覧、2: カレントディレクトリ取得（パス引数は無視）、3: 環境変数取得、4/5: メタデータ取得（4 はシンボリックリンクを追跡、5 は追跡しない）。`__write` は 0: 新規作成・切り詰め、1: 新規作成・追記、2: mkdir、3: rmdir、4: unlink。`__open` は 0: Read、1: Write、2: Append、3: CreateNew。`__handle` は 0: read、1: write、2: flush。`__clock` は 0: モノトニック時計、1: Unix エポック時です（単位はナノ秒、取得失敗時は `INT64_MIN`）。
戻り値の状態ステータスは `i64` であり、成功時は 0、失敗時は `(kind << 32) | (errno & 0xffffffff)` となります。エラー種別 `kind` は 1: NotFound、2: PermissionDenied、3: AlreadyExists、4: InvalidInput、5: InvalidEncoding、6: Interrupted、7: Other（`Os.ErrorKind` の定義順）であり、std の `Os.error_of_status` がこれを `Os.Error { kind, code }` 構造体へと復元します（Tsuzuri 自身が検出したエラーの `code` は 0 となります）。LLVM は `status == 0 || (status >> 32) - 1 < 7` の妥当性を検証し、範囲外の値に対しては `read_line` と同様に `BoundsCheck` トラップを発生させます。`__open` は正のファイルハンドル値または負のエラー状態値を返すため、負の場合にのみ符号を反転して検証を行います（`__clock` はステータス検証を行いません）。
バイト列を返すプリミティブ（`__read`、`__args`、`__random`、`__handle`、`__spawn`）は、IO と同一の `host_result_slot`／`read_host_result` 記述子を先頭の出力先ポインタとして受け取り、成否にかかわらず結果を書き込みます（失敗時は NULL ポインタと長さ 0）。LLVM はその記述子と状態値から `(i64 * [ubyte])` のタプルを構築し、所有バッファは通常のアロケータによって安全に `drop` されます。`ref utf8string` はスタック記述子のポインタと長さへ、共有の `ref [ubyte]` は記述子の値そのもの（ポインタと長さ）へと展開して渡されます。

**OS API のリンクと入口:** 到達可能性解析によって実際に到達したプリミティブのみを `declare i64 @tsuzuri_os_<name>(...)` として抽出し、`BTreeSet` で管理される intrinsic 集合に追加するため、外部宣言の重複は発生せず出力は完全に決定的となります。OS API を使用しないプログラムでは、生成される IR やランタイムの構造は一切影響を受けません。`Os.__args`（`Env.args`）に到達した場合にのみ `declare void @tsuzuri_os_set_args(i32, ptr)` も追加宣言され、この宣言が存在するときに限り、C のエントリーラッパーが `@main(i32 %argc, ptr %argv)` となり、`tsuzuri_main` の呼び出しに先立って argc と argv が保存されます（なお、`Env.args` に argv[0] は含まれません）。それ以外のプログラムの入口は従来の引数なし `@main()` のままです。`@tsuzuri_os_` を含む IR に対しても、アロケータは `tsuzuri_alloc` および `tsuzuri_free` を適切に公開します。
ネイティブの `src/runtime/os.c` は、IR が `declare i64 @tsuzuri_os_` を含んでいる場合にのみ、ドライバーによって C ランタイムの単一コンパイル単位（translation unit）へと組み込まれます。連結順序は `os.c`、`trap.c`、タスクランタイム、`cpu.c`、`io.c`、`arguments.c` の順であり、`os.c` を先頭に配置するのは `_DARWIN_C_SOURCE` や `_GNU_SOURCE` を先頭の `#include` よりも前に定義する必要があるためです。公開される関数は weak かつ hidden 属性を持ちます。
`IO<i32>` を返すトップレベルエントリーポイントでは、戻り値の整数を解放（drop）せずに `tsuzuri_main` の戻り値、すなわちプロセスの終了コードとして直接返却します。`llvm::exit_code_entry` が `def main` と std の `IO<i32>` の組み合わせを判定し、それ以外の `IO<T>` では結果を drop して正常終了コード 0 を返します（Tsuzuri に `Os.exit` 関数は存在しません）。`tsuzuri run` は、`exit_code_entry` を持つネイティブプログラムが非ゼロで終了した際、トラップエラーとは明確に区別して `E2005`（`program exited with code N`）を報告します（JSON 診断モードでは標準エラー出力の内容も添付されます）。

**`def main` の入口:** `llvm::main_entry` によるエントリーポイントでは、コンソール用の `@main` が `Main.main` を `i8 0`（`unit`）または `%tz.array` の引数で呼び出し、戻り値の `i32` をそのままプロセスの終了コードとして返します（コンソールへの値の表示は行いません）。
`Array<string>` を受け取る `main` 関数では、ラッパー関数が `@main(i32 %argc, ptr %argv)` として定義され、`declare void @tsuzuri_arguments(i32, ptr, ptr)` によって生成された `%tz.array` を所有権ごと渡します（呼び出された callee 側が配列を drop します）。もし `Env.args` にも到達している場合は、それに先立って `tsuzuri_os_set_args` を呼び出します。
ドライバーは、IR 内に `@tsuzuri_arguments(` が含まれている場合にのみ `src/runtime/arguments.c` を C ランタイムのコンパイル単位の末尾に追加し、IR 側は公開アロケータ（`tsuzuri_alloc`／`free`）を提供します。各引数は `tsuzuri_alloc` によって確保された UTF-16 の文字列（`%tz.string`）となります。
POSIX 環境では argv[1..] の UTF-8 文字列をデコードして格納し（不正なバイトシーケンスは U+FFFD に置換されます）、Windows 環境では `GetCommandLineW` から取得した文字列を空白およびタブ文字で分割します。ダブルクォーテーションで囲まれた範囲（例: `"b c"`）は内部の空白で分割せずに 1 つの引数として扱い、引用符自身は除去され、バックスラッシュは特別扱いされません。なお、先頭の実行プログラム名（argv[0]）は破棄されます。
ライブラリ出力モードでは `tsuzuri_main()` が定義され、ネイティブでは空の引数配列が渡されます。WASM 出力では IR 内の弱いシンボルである `tsuzuri_arguments`（空配列を返す）が呼び出され、`--wasm-host wasi` 指定時には `os-wasi.c` の後に `arguments.c` を結合した強いシンボル定義が `args_sizes_get` および `args_get` を通じて実際の引数を取得します。
`exit_code_entry` は `def main` を含むため、`tsuzuri run` における `E2005` の報告や WASI の `proc_exit` 呼び出しは `IO<i32>` のエントリーポイントと同様に機能します。`tests/arguments.rs` と `tests/arguments_runtime.c` が引数の分割・デコードと実行ファイルへの受け渡しを検証し、`tests/os.mjs` がネイティブおよび WASI での引数処理を検証しています。

**OS API と wasm・Windows:** WASM のデフォルト出力では、`tsuzuri_io` 以外のホストインポートを持たない独立バイナリの生成を原則としています。そのため、OS プリミティブに到達した WASM オブジェクト、LLVM IR、およびスタンドアロン WASM の出力は、ファイル生成前に `E2000`（`OS_WASM_MESSAGE`）エラーとして拒否されます（ただし、`Path` の操作、`Os` 内の純粋な補助関数、および `Random.Pcg` はプリミティブを呼び出さないため、インポートなしで安全にビルド可能です）。また、Windows 上でコンパイラを動作させてネイティブの実行ファイルやオブジェクトを生成する際、OS プリミティブに到達している場合は `E2002`（`OS_WINDOWS_MESSAGE`、`--emit llvm` を除く）エラーとなります（G10 における Windows ネイティブの完全な実行環境は未検証であるため、現時点では対応を公称していません）。
`--wasm-host wasi`（`BuildOptions.wasm_host`。ビルド時のみ指定可能）は wasm32 のオブジェクトおよびスタンドアロン WASM 出力でのみ指定可能であり、`--wasm-feature threads` や `--emit llvm` との併用は拒否されます（`E2000`）。これは、`os-wasi.c` をオブジェクトとしてコンパイルして静的リンクする構成をとっているため、IR のみを出力するモードでは `tsuzuri_os_*` や `tsuzuri_io_*` の未解決シンボルが残存してしまうためです。`llvm::with_wasi_host` は標準 IO の宣言から `tsuzuri_io` のインポート属性を除去し、`src/runtime/os-wasi.c`（freestanding C11）を別オブジェクトとしてコンパイルして結合します。`os-wasi.c` は `os.c` や `io.c` と同一のインターフェイスを `wasi_snapshot_preview1` 上に実装しており、到達した関数に対応する WASI インポートのみが追加されます。ファイルパスは preopen されたディレクトリのうち `/` 境界で最長一致するプレフィックスへ解決され、いずれにも一致しない場合は最初の preopen ディレクトリからの相対パスとして扱われます。`Os.Error.code` には WASI の errno が格納され、`Env.current_dir` は最初の preopen ディレクトリ名となり、`Process.run` は `Other`（未対応）エラーとなります。`_start` は `--emit wasm` 指定時、かつエントリーポイントが `def main` または IO である場合にのみ定義され（`-DTZ_WASI_START`）、`def main` や `IO<i32>` の非ゼロの戻り値は `proc_exit` へ引き渡されます（`-DTZ_WASI_EXIT_CODE`）。オブジェクト出力時の `_start` の定義はホスト側に委ねられます。なお、WASI preview2 および WebAssembly コンポーネントモデルは現時点で未実装です。

**ファイルハンドルと子プロセス:** `File.Handle { id: i64 }` は、ランタイム内部のファイルテーブルのエントリを指す不透明な Copy 値であり、あえて `Drop` を実装していません（標準ライブラリの型は言語規則上 `Drop` を勝手に実装できず（`E1016`）、また `Drop` を持つ値は `let!` の継続境界を越えられないという言語仕様（`E1005`）が存在するためです）。ハンドルの内部値は `(generation << 32) | (slot + 1)` としてエンコードされ、ファイルをオープンするたびに一意で再利用されない世代番号（generation）がインクリメントして割り当てられます（世代が $2^{31}-1$ 回に達した場合は `EOVERFLOW` 起因の `Other` エラーとなります）。クローズ済みまたは古い世代のハンドルを用いてアクセスした場合、同じスロット記述子番号を再利用している別のファイルへ誤ってアクセスすることはなく、確実に `InvalidInput`（EBADF）エラーとなります。ファイルテーブルは必要に応じて倍々に拡張され、すべてのオープンファイルがクローズされると自動的にメモリが解放されます。`File.with_open` を使用すれば、例外や早期リターンを含むすべての実行経路で確実にファイルがクローズされます。ファイルは常に `O_CLOEXEC` フラグ付きで安全に開かれ、ディレクトリをファイルとして開こうとした場合は `InvalidInput`（EISDIR）で即座に拒否されます。
`Process.run` は、シェルの介在を挟まずに `posix_spawnp` を直接用いて子プロセスを安全に起動します。引数列は NUL 文字区切りで渡され、子プロセスの argv の各要素へと確実に展開されます。標準入力データはパイプ経由で書き込まれた後に速やかに閉じられ、標準出力および標準エラー出力は `poll` と非ブロッキング読み取りを組み合わせて並行して回収されます。出力データの合計サイズが $2^{30}$ バイト（1 GiB）を超過した場合は、リソース枯渇を防ぐため子プロセスへ `SIGKILL` を送信して強制終了させ、`Other` エラーを返します。子プロセスへの書き込み中に発生した `SIGPIPE` シグナルは、その関数呼び出しの実行中のみ安全に無視されます。

**OS API の検証:** `tests/os_api.rs` では、予約名、非公開プリミティブの不可視性（`E1022`）、型整合性、到達した関数のみが外部宣言・ランタイムに含まれること、argv を受け取るエントリーポイントの挙動、`IO<i32>` の終了ステータス、ならびに `Random.Pcg` と `File.Handle` の不透明性を包括的に検証します。`tests/os.mjs` では、ネイティブおよび WASI（`tests/os-wasi-host.mjs` を通じて `node:wasi` 上で実行）の双方を `-O0` および `-O3` で動作させ、実際のファイルシステム、環境変数、コマンドライン引数、終了ステータス、ならびに `E2000`／`E2005` 診断の発生条件を Node.js の `fs` および `os` モジュールの期待動作と厳密に照合します。ネイティブと WASI の実行結果は、OS 固有の errno 番号の差異を除いて完全に一致します。またメモリ追跡ハーネスを用い、生成 IR 内の `@malloc`／`@realloc`／`@free` および `os.c` 内部での libc 呼び出しをインターセプトして計数し、ASan／UBSan 有効環境で 64 回の連続反復実行を行っても、生存メモリブロック数が完全に 0 に復帰することを検証しています。なお、OS API の処理速度については特定の性能主張を行っていません。

**定数:** `Program.constants` で定義された定数は、内部的な引数なし関数宣言として収集され、通常の関数と完全に同一の名前解決、可視性判定、および型検査のルールが適用されます。
特殊化や所有権検査に先立って、明示的なスタックを用いた依存解析が行われ、定数の評価結果はキャッシュされて参照箇所へと型付きリテラルとしてインライン展開されます。
型検査時には条件分岐の双方が通常の静的型検査を受け、定数評価器のみが短絡評価を行います。2進（binary）浮動小数点は `rustc_apfloat`（APFloat）を用いてビット幅ごとに正確に計算され、decimal 演算のコンパイル時評価は拒否されます。
定数の内部宣言は非公開とされ、出力コードの到達性ルートからは除外されます。コンパイラコアに独立した定数ランタイムや共有所有メモリ領域を新設することはせず、既存の文字列リテラル用グローバル領域、集約値の生成機構、およびフレーム管理／ヒープ移送／デストラクタ（frame/relocate/drop）の仕組みをそのまま共有します。
定数や式の一時値（関数呼び出しやパイプライン演算の結果など）を共有借用（`ref`）の引数へ渡す際は、`constants::temporary_borrows` がそれらを `BorrowOperand` に変換して一時値を生成・呼び出し後に安全に解放し、型検査によって「単一段階の完全適用であること」および「戻り値に借用が漏洩しないこと」を保証します。
動作は `cargo test --locked --test constants` および `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri constants` で検証されています。

**デバッグ情報:** `llvm::emit_with_debug_info` は既存の `TrapSource` ソースマップを明示的に受け取り、DWARF デバッグ情報を付与したコードを生成します（通常の API 呼び出しではデバッグ情報は生成されません）。
`DIFile`、`DICompileUnit`、`DISubprogram`、`DILocation`、ならびに変数や型のメタデータノードは `Globals.next_metadata` によって一元的に採番され、ループメタデータ、トラップマーカー、および同梱ランタイムと同一の ID 空間を共有します。
ソース位置情報は既存の `current_span` を利用し、`switch` の case 行ではなく各命令の終端位置に正確に関連付けられます。ローカル変数のエントリーブロックにおける `alloca` の直後には `llvm.dbg.declare` が配置されます。
公開 ABI やコンソール用ラッパー関数にも適切なスコープ情報が付与され、`-O3` 最適化によって内部関数がインライン展開された場合でも元のソース位置情報が確実に保持されます。

DWARF の形は、formatter を読み込まないデバッガーでも Tsuzuri の名前が見え、`scripts/lldb/tsuzuri_lldb.py` が値を復元できるように決めています（G16）。命令列は変えず、`-g` なしの IR は同一です。

- 型名は `Type::display`（`i64`、`[|i64|]`、`Maybe<i64>`、`Main.Shape`）です。`bool` 以外のスカラーは同名の基本型への `DW_TAG_typedef` です（デバッガーは基本型には C の名前を、typedef には typedef の名前を表示するため）。`char`・`utf8char` の基本型は 16・32 bit の `DW_ATE_UTF`、2進浮動小数点は `DW_ATE_float`、decimal 型は BID のストレージとして `DW_ATE_unsigned` です。名前の付くポインター（参照、ハンドル、`Rc`・`Arc`、再帰 union）も、名前のないポインターへの typedef です。
- リストの `head` はノード構造体 `<型名>.node`（`next` と `value`。`FunctionEmitter::list_node_type` の `{ ptr, T }`）へのポインターです。関数値と Task の `code` は関数型へのポインター、`environment`・`clone`・`drop` と dyn 値の `data`・`vtable` は型のないポインターです。
- union はすべての case が値を持たなければ `DW_TAG_enumeration_type` です。そうでなければ構造体で、メンバー `$tag`（列挙型 `<型名>.$tag`）と `$payload`（値を持つ case ごとに case 名のメンバーを持つ `DW_TAG_union_type` `<型名>.$payload`）を持ちます。`$payload` の offset は `union_layout` が `Common(T)` なら 4 を `T` の align に切り上げた値、`General(_)` なら 16 です。再帰 union はノード構造体 `<型名>.node`（`next`・`drop`・`clone`・`$tag`・`$payload`。`emit_program` の `{ ptr, ptr, ptr, i32, payload }`）へのポインターの typedef で、空ポインターは最初の値を持たない case です。`$` は Tsuzuri の識別子に使えないので、record のフィールドと衝突しません。
- `DISubprogram` には `linkageName` がなく、`name` は `CheckedFunction::qualified_name` から単相化の `.$mono.<N>` を除いた名前（`Main.show`、全実体が `Array.sum`）です。ラムダ式と Task は `<外側の関数>.lambda@<行>:<列>`・`.task@<行>:<列>`、callback の特殊化は元の関数と同じ名前です。`export` の `@tz_<name>`、`@tsuzuri_main`、`@main` などのラッパーは、シンボル名と `DIFlagArtificial` を持ちます。組み込み関数・組み込みメソッド・case のコンストラクター・export の bridge（モジュール `$builtin`・`$intrinsic`・`$case`・`$export`）は自分のソースを持たないので、`DISubprogram` を付けません。
- `let` の束縛の `store` は局所変数の宣言の位置に、閉包の捕捉の読み出しは閉包の位置に置きます。引数の `store` は位置を持たず、`DISubprogram` は `scopeLine` を持たないので、`loop` ブロックで引数を束縛するまでの prologue は行 0 になり、デバッガーは関数の本体の最初の行で止まります。
- ランタイムの C（`task.c` など）はデバッグ情報なしでコンパイルし、生成する補助関数（`tz.apply.*`、`tz.drop.rec.*`、`tz.clone.*`）にも位置を付けません。LLDB は既定でデバッグ情報のない関数へステップインしないので、ステップ実行は Tsuzuri のソースだけを辿ります。プログラムの DWARF はコンパイラーの compile unit（DWARF 4）だけになり、Clang の既定の DWARF 5 の runtime の unit を `llvm-link` で結合したときにプログラム全体が DWARF 5 になって、LLVM 21 の `-O3` の `.debug_names` が `llvm-dwarfdump --verify` を通らなかった問題もなくなります。
- Windows で MSVC のリンカーを使うとき（`driver::msvc_linker`。配布物の `tsuzuri-clang` は MinGW の `ld.lld` でリンクするので除く）、driver は `-g` のネイティブの IR に `llvm::with_codeview` で `CodeView` の module flag を足し、LLVM は DWARF と並べて CodeView（`.debug$S`・`.debug$T`）を出します。実行ファイルと `tsuzuri test -g` のランナーのリンクには `-Xlinker` で `/PDB:<一時ディレクトリ>`、`/PDBALTPATH:<出力名>.pdb`、`/NATVIS:`（`src/runtime/tsuzuri.natvis`）を渡し、PDB を `<出力の拡張子を .pdb にした名前>` の sidecar として公開します。`lld-link` では PDB と DWARF の両方が残ることを確かめています（G16 Phase 3）。
- formatter の正本は `scripts/lldb/tsuzuri_lldb.py` で、LLDB の `lldb` モジュールだけを使い、型名と `$tag`・`$payload` のメンバー名で型を見分けます。`scripts/toolchain/bundle.mjs` が配布物の `share/lldb/` へ、`vsc/scripts/toolchain.mjs` が VS Code 拡張の `resources/lldb/` へ複製し、拡張はデバッグの `initCommands` の先頭で `command script import` します。

Clang には `-g` フラグが渡され、WASM リンク時の `--strip-all` は解除されます。macOS におけるデバッグタスクオブジェクトの生成では、対応する `llvm-link` を用いて事前に IR を結合してから単一オブジェクトとして生成することで、`ld -r` による DWARF 情報の脱落問題を回避しています。
また macOS のデバッグ実行ファイルは、保持された `module.o` からリンクを行い、一時ファイルを削除する前に `dsymutil --flat` を実行して隣接する `output.dwarf` を生成します。
DWARF メタデータとトラップ情報テーブルは、共通のソースファイル保護、バックアップ、および公開失敗時のロールバック機構を利用して安全に出力されます。
なお、Cargo の release プロファイルにおける strip 設定はコンパイラ自身のバイナリにのみ適用され、生成対象のバイナリには干渉しません。検証は `tests/debug_info.rs`（名前、束縛の位置、スカラーとポインター、union の形と offset、テストランナーの `tsuzuri_test_run` の subprogram）、`tests/debug_info.mjs` によるネイティブおよび WASM の `-O0`／`-O3` テストと `llvm-dwarfdump --verify`、ならびに LLDB の batch で表示・名前・ステップ実行と `-O2` のテストランナーのブレークポイントを確かめる `tests/debugger.mjs`（LLDB が必要なので共有 CI では実行しません）によって行われています。

**言語サーバー:** `tsuzuri lsp` は、LSP 3.17 仕様に準拠した stdio フレームプロトコルと `serde_json` を使用し、最大メッセージサイズ 16 MiB、ヘッダー長 8 KiB、JSON 再帰深度 128 を上限として安全に動作します。
文字位置のエンコーディングはクライアントとのネゴシエーションによって決定され（デフォルトは UTF-16）、バイト位置（byte span）からの相互変換には改行テーブルと文字境界判定を用いて、CRLF 改行やサロゲートペア・異体字セレクタ等の補助平面文字を正しく取り扱います。
クライアントから受信したドキュメント URI の表記揺れはそのまま内部で保持され、診断情報や定義ジャンプの応答において正確にクライアントへ返却されます。
`Project::load_with_overlays` は、エディタ上で未保存の編集内容や新規作成ファイルをインメモリオーバーレイとして反映し、ディスク上のソースファイルを書き換えることなくプロジェクト全体を再解析します。
ワークスペースのルートは `workspaceFolders` または `rootUri` に基づいて決定され、未指定の場合は各ファイルの親ディレクトリが使われます。ネストしたルート指定が存在する場合は最長プレフィックスが選定されます。
ファイル変更通知は 200 ms のデバウンスウィンドウで集約され、クライアントからのリクエスト受信時には必要な解析を先行して完了させます。開かれているバッファの合計サイズは 32 MiB、ファイル数は最大 1024 件に制限されています。
リーダー（受信）スレッドは有界キューを通じてリクエストを受け取り、リクエストごとのキャンセル ID を共有管理します。重い解析の開始前および応答の送信前にキャンセルの有無を確認し、標準出力には厳密にフォーマットされた JSON-RPC メッセージのみを出力します。
`analyze_modules_with_semantics` は、通常の解析結果に加えて `SemanticIndex` を構築して返します。このインデックスは型検査の完了後、定数展開・単相化・クロージャの lowering より前の段階で採取され、通常のコンパイル時には生成されません。
後続の所有権検査などでエラーが検出された場合でも不完全なインデックスを公開することはなく、以前の正常なインデックスも安全に破棄されます。サーバー機能としては、ホバー（hover）、定義ジャンプ（definition）、およびシンボル検索（symbol）を中心とした堅牢な機能を宣言・提供します。
動作は `cargo test --locked --test lsp` および `cargo build --release --locked && node tests/lsp_sessions.mjs target/release/tsuzuri` によって検証されています。

**REPL:** `tsuzuri repl` は再コンパイル型で、JIT も入力を跨ぐ常駐プロセスも持ちません（G13 D1・D13）。セッションは受け付けた宣言とトップレベルの文の**ソース**だけで、入力ごとに「宣言 → 文 → 入力の末尾」の順に `Main.tz` を生成し、`Project::single_main` で読みます。これはファイル・マニフェスト・lockfile を読まず、`stdlib::sources_for` が選んだ std だけを足すので、同じ文字列を `Main.tz` に置いた `tsuzuri run` と同じ検査・IR になります。生成したソースはディスクに書かず、パスは仮想の `<repl>/Main.tz` です（キャッシュのキーとトラップの行に入る）。
式は `let it = (E)` と `Display.display (ref it)` の版を先に検査し、`SemanticIndex` の `it: T` の項目から型を得て値を表示します。その版が通らないときだけ `let it = (E)` の版を検査し直し、通らなかった理由が表示行の `E1005` だけなら型だけを表示します（表示できる式の解析は 1 回。G13 Phase 3）。`run_captured` は `run` と同じ `build_complete` の後、子の stdin を空にし、stdout を 16 MiB まで別スレッドで集め、stderr を別スレッドで中継し、時間制限か出力の超過で子を kill して `wait` してから `E2005` を返します。`run_with_diagnostics` はこの `run_process` の `Inherit` で、`run` の挙動（stdio の継承、stderr の中継、`--json`、`E2005` の位置）は変わりません。
動作は `cargo test --locked --lib repl::` と `cargo build --release --locked && node tests/repl.mjs target/release/tsuzuri` によって検証されています。
1 入力の待ち時間（`-O0` の新しい式で約 0.67 秒）は、解析、cache の鍵（ツールの `--version` とコンパイラの SHA-256）、Clang のコンパイルとリンク、新しい実行ファイルの初回の起動（macOS の検査）でほぼ 4 分されます（`node benchmarks/run-repl.mjs`、[性能測定](benchmarks.md#repl-の-1-入力の待ち時間g13)）。JIT（LLVM の C API の ORC）、`lli`、入力ごとの共有ライブラリを読み込む常駐ホスト、値のスナップショットは、`unsafe`・新しい crate・LLVM ライブラリの配布・値の ABI を要し、この節の方針と衝突するので採りません（G13 D13）。

**スクリプト実行:** `tsuzuri script FILE [arguments...]` は `run` と同じ経路で、`Project::load_script` が FILE（名前は問わず、シンボリックリンクは辿る。`.tt`・`.tc` は `E2000`）を `Project::single_main` の `Main.tz` にする。FILE の実際のパスを保つので、診断・トラップの行・キャッシュのキーはそのパスを使い、同じスクリプトの再実行はキャッシュに当たる。実行ファイルの入口の検査（`E2004`）はパスのファイル名ではなく入口のモジュールの相対パス（`Main.tz`）を見る。FILE より後ろの引数は `run_with_arguments` が子プロセスへそのまま渡す（`run` は空）。shebang 行は lexer の規則で、`tsuzuri fmt` は行をそのまま出力の先頭に写す。動作は `cargo test --locked --lib skips_a_shebang`、`--test formatter keeps_a_shebang_line`、`--test lsp a_shebang_line_keeps_positions` と `node tests/script.mjs target/release/tsuzuri` によって検証されています。

**フロントエンド:** プロジェクト内の全ファイルのシグネチャを先行して収集するため、宣言の記述順序やファイル順序に依存しない設計となっています。
ローカル変数の束縛は一意な識別子（ID）へと解決され、コード生成フェーズにおいて名前解決や型推論をやり直すことはありません。
呼び出し実引数に対する借用・参照外しの自動補完（後述）を除き、暗黙の型変換、未知の識別子に対する既定値の補填、または未対応構文のスキップなどは一切行いません。
決定性が求められる名前の集合には、順序が保証されたコレクション（`BTreeSet` や `IndexMap` など）を使用します。
ソースの位置情報（Span）はソースファイル ID とファイル内バイトオフセットを正確に保持し、字句解析、構文解析、型検査、メモリレイアウト計算で発生したすべてのエラーを元のファイル上の正確な位置へ対応付けます。ソースコードの結合や診断オフセットの事後補正などは行いません。

`analyze_all`、`analyze_modules_all`、`analyze_modules_with_std_all`、および `Project::analyze_all` は複数件の診断結果を格納した `DiagnosticSet` を返し、従来の単一診断 API はこの集合の先頭要素を返すラッパーとして維持されています。
診断収集構造体 `Diagnostics` は、`(source.unwrap_or(root), start, end, severity, code, message)` をキーとして自動的に重複を除去します。
診断の収集上限（1000 件）と表示上限（50 件）を分離し、表示上限に達した場合でも収集打ち切り前であれば正確な省略件数を算出し、収集自体が上限に達した場合にのみ下限値として表示します。
なお、診断メッセージに付随する補足注記（note）は独立した Diagnostic オブジェクトや診断コードを持ちません。ユーザーコードと標準ライブラリのソース ID 順序は一貫して維持されます。

警告（warning）は型検査完了後かつ単相化の前に `CheckedModule.warnings` へ集約され、致命的なソースエラーが存在する場合には表示が抑制されます。
識別子の由来情報 `Ident.provenance` は `Local` 変数へ引き継がれ、未使​​用変数の警告 `W1001` はローカル ID の使用履歴から、未使用プライベート関数の警告 `W1002` は再帰検査と共有されるコールグラフおよび型参照の到達性解析から判定されます。
型別名の走査では、型消去前の注釈情報も走査対象に含まれます。標準ライブラリおよびコンパイラが自動生成した内部束縛に対する警告は抑制され、生成された識別子の文字列パターンから安易に由来を推測することはありません。
網羅性の警告 `W1003` は後述のパターン網羅性解析から生成され、シャドーイング等の字句スコープ警告 `W1004` はデフォルト無効の検査として実装されています。`--deny-warnings` オプションを指定した場合は、コード生成へ進む前にビルドを失敗させます。
暗黙コピーの警告 `W1006` は型検査の後段で生成されます。`copies::sites` が具体化およびクロージャ降格後のモジュールに対して所有権チェッカーを収集モードで再実行し、消費的な読み取りの中でデストラクタ呼び出しを要する Copy 値の複製箇所（単一使用のローカル変数の move を除く）を、位置・型・複製種別とともに抽出します。
`main.rs` は `--warn implicit-copy` が指定されている場合にのみ、配列やリストの暗黙複製を警告として報告し、LSP は同一の複製情報を `textDocument/inlayHint` のインレイヒントとしてエディタへ返却します。
デバッグビルド時の LLVM emitter は、ユーザーコードの式から発生した暗黙の `clone_value` 呼び出しをすべて記録し、そのすべてが事前の一覧に含まれていることをコード生成の最終段階でアサートします。

テスト宣言 `Program.tests` は通常の名前空間から隔離され、検査時に unit を返す内部呼び出し可能関数および `CheckedModule.tests` のメタデータへと変換されます。
テスト関数由来の属性 `FunctionOrigin.test` は局所的に生成された補助関数へも伝播され、共有されるジェネリック関数の特殊化実装は 1 つだけ生成されてルート集合からの到達性によって取捨選択されます。
通常ビルドでは公開宣言、エクスポート関数、エントリーポイントが到達性のルートとなりますが、テストビルドでは選択されたテスト関数群がルートとなり、同一の `reachable_functions` アルゴリズムが適用されます。
テスト用内部関数のシンボル名には `@tz.fn.Module.$test.index` という決定的な命名規則が用いられ、テストランナーのみが `tsuzuri_test_count` および `tsuzuri_test_run` を公開関数としてエクスポートします。
ネイティブの C エントリーポイントは `strtoull`、errno、および endptr を用いて指定されたテストインデックスを厳密に検証し、WASM 側の Node.js エントリーポイントはインポートが空であることを確認します。
`driver::run_tests` はテストランナーを 1 回だけビルドし、上限付きの並列ワーカープロセスを用いて各テストを独立したサブプロセスとして実行します。30 秒のタイムアウトに達したテストプロセスは安全に終了・待機され、実行結果は元のテスト宣言順序へ並べ直されて出力されます。
`tsuzuri test --index N -g -o PATH` の `driver::build_debug_runner` は、テスト N だけをルートにした IR を `emit_test_runner_with` の `TestRunnerOptions::debug` で DWARF 付きにし（テストの本体の名前は `<モジュール>.test@<行>:<列>`。テストを呼ぶ `@tsuzuri_test_run` は `@main` と同じく artificial な `DISubprogram` を持ち、呼び出しに位置を付けるので、`-O1` 以上でテストが inline されても行が残ります）、C の入口とランタイムはデバッグ情報なしの別オブジェクトにして `-g` でリンクし、macOS では `dsymutil --flat` で `PATH.dwarf` を作って `publish_outputs` で置きます。実行はせず、ランナーの絶対パスと引数 `0` を出力します（G16 Phase 2）。VS Code 拡張の Debug のテストプロファイルはこれを CodeLLDB で起動し、終了コードで成否を報告します。

`tsuzuri test --coverage`（G18 Phase 2）は LLVM のカバレッジ形式や gcov を使わない自前の計装です。`coverage::plan` が単相化・ラムダ持ち上げ後の利用者関数（標準ライブラリ、`FunctionOrigin.test` を持つテスト本体とその補助関数、組み込み・ケース・export のラッパーを除く）の型付き本体を走査し、関数本体・`if` の両枝（`else` のない `if` が生成する `()` は除く）・`match` の節の本体・ループ本体・`try` のハンドラー・`&&`／`||` の右辺を (ソース, span, 種類) の昇順に番号付けした region と、各式の開始位置とその時点で最も内側の region の組（point）を作ります。
`llvm::emit_test_runner_covered` はこの計画を `Globals.coverage` に置き、`FunctionEmitter::cover` が region の block の先頭（`&&`／`||` の右辺は `short_circuit` が右辺を評価する block の先頭）で `atomicrmw add ... monotonic` を `@tsuzuri_coverage_counters` の要素へ出します（同じ span の特殊化はカウンターを共有し、`lookup_match` の定数表は使いません）。呼び出しを省く生成（恒等関数の呼び出しを引数に置き換える `call_specialization::is_identity`、既知のクロージャの解決で恒等関数を見通す `transparent`、末尾の自己呼び出しの引数で `x + y`／`x - y` だけの 2 引数関数を加減算にする `tail_arguments`）は、`FunctionEmitter::cover_call` で呼ばれる側の本体の region をその場で数えるので、呼び出し側がテストでも数は呼び出しと同じです。計画がない出力は 1 byte も変わりません。
C の入口は `-DTSUZURI_COVERAGE` のときだけ、テストが成功したらカウンターを `TSUZURI_COVERAGE_FILE` へ native の byte 順で書きます。`driver::run_tests` は成功したテストのファイルだけを飽和加算で合算し（失敗はカウンターを書く前に終わるので除外数として数える）、`coverage::files` が行ごとに point の region の最大値を取り、`render_lcov` が lcov を書きます。

ベンチ宣言 `Program.benches`（G18 Phase 1）はテストと同じく通常の名前空間から隔離され、`$bench.<index> ($iterations: i64) -> i64 = { let $case: i64 -> i64 = 本体; $case $iterations }` という生成関数と `CheckedModule.benches` になります（本体の型の不一致は `$case` の注釈により本体の位置の `E1003`）。由来 `FunctionOrigin.bench` は補助関数へ伝播します。
`polymorph::specialize` はベンチ関数の要求を、ほかのすべての特殊化（Drop の固定点を含む）が終わった後に出すので、通常ビルドとテストビルドの `$mono.N` の番号はベンチの有無で変わりません。この後半で初めて見つかった Drop 型の drop 関数は `CheckedModule.bench_drops` に記録され、`reachable_functions` はベンチ実行器のときだけそれらを根にします（ベンチ専用の型の drop glue は通常の成果物に出ません）。ただし `$lambda.N` や `$instance.N` のように関数の総数から付く生成名は、テストを足したときと同じく、ベンチや `Bench` モジュールの読み込みで番号がずれます。
`tsuzuri bench` 以外の出力（根を指定しない Console・Library）では、`program_reach` が根の候補（公開の利用者関数、export、入口）からの到達集合を求めるときに `Bench.now` を直接読む関数を記録し、あれば逆向きの到達で時計に届く関数を求めて、export でも入口でもないものを根から外してもう一度到達集合を求めます。外した関数がなお到達されるとき（入口・export・選んだテスト・Drop から）は、`clock_error` が根から時計への最短の経路を幅優先で求め、起点と途中で最後に通る利用者の関数を名前で示し、その関数の中で次へ向かう式の位置に `E1018` を出します。根の候補が時計を読む関数に到達しないプログラムでは、到達の計算と出力は変わりません（到達するプログラムは以前は必ず `E1018` だったので、ビルドできたプログラムの出力も変わりません）。
`llvm::emit_bench_runner`（`Entry::BenchRunner`）は選んだベンチ関数を根にして、`@tsuzuri_bench_count` と、index で分岐して `call i64 @tz.fn.<Module>.$bench.N(i64 %iterations)` を返す `@tsuzuri_bench_sample(i32, i64)`（不正な index は -1）を出します。組み込みの `Bench.now` は `call i64 @tsuzuri_bench_now()` に下がり、宣言がベンチ実行器以外の出力に残れば `emit_program` が `E1018` を返します。`Bench.consume` は値を entry block の alloca に store し、その pointer を `asm sideeffect "", "r,~{memory}"` に渡してから `drop_value` します。
`runtime/bench-runner.c` は `tsuzuri_bench_now`（macOS は `clock_gettime_nsec_np(CLOCK_UPTIME_RAW)`、Linux は `_POSIX_C_SOURCE` 付きの `CLOCK_MONOTONIC`、Windows は `QueryPerformanceCounter`）と、引数 `INDEX SAMPLES TARGET_NS` を厳密に検査して（不正なら終了コード 2）反復回数を倍々に較正し、予熱の後に `iterations N` と `sample NS` の行を出す `main` を持ちます（負の時間は終了コード 3）。
`driver::run_benches` はテストランナーと共有する `compile_native_runner`（C の入口と必要なランタイムをリンク）で実行器を 1 回だけ作り、ベンチを 1 件ずつ別プロセスで順に実行します。標準出力と標準エラーは読み取りスレッドが読み切り（パイプが満杯でも子を止めない）、300 秒で kill と wait をします。出力の形と件数を厳密に検査し、1 回あたりの時間（`ns / n / 1e6` ms）の中央値・最小・最大を報告します。合否の閾値はありません。

プロパティテスト（G18 Phase 3）は std の opt-in モジュール `Gen` だけで書かれています。`Gen<'a>` は `Source -> ('a * Source)` を持つ不透明なレコードで、`Source` は `(Random.Pcg * [i64u] * bool * Vec<i64u>)`（乱数、再生する選択、再生中か、記録した選択）の組です。生成器はすべての乱択を `choose`（上限 0 の選択は記録しない）で記録し、探索では seed と値の番号を stream にした `Random.pcg` の提案を、再生では記録（尽きたら 0）を上限で切った値を使います。`shrink` は記録を短縮・縮小した候補を再生し、性質が失敗し続けてかつ短長辞書順でより単純な記録だけを採用します。
seed は std 専用の組み込み `Gen.__seed()` で、`emit_builtin` が `Globals.seed`（`llvm::TestRunnerOptions.seed`、なければ `DEFAULT_PROPERTY_SEED`）の定数を返す関数を出します。`Gen` を書かないプログラムでは読み込まれないので、型検査と IR は変わりません。
テストランナーは `run_captured` で子プロセスの標準エラーを読み取りスレッドで読み切り（末尾 64 KiB を保持）、失敗したテストの `TestResult.output` にします。WASM のテストランナーは、`Gen` の関数を含むモジュールだけを `debug_output` 付きで出力し、`runtime/test-runner.mjs` は `tsuzuri_debug.write` だけをインポートとして許して `writeSync(2, ...)` で書きます。

`Debug.print` および `Debug.trace` は通常の標準ライブラリ関数として提供され、`Display` が生成した所有文字列を非公開の組み込み関数へと渡します。
ネイティブ環境では厳密な UTF-8 変換と `runtime/debug.ll` の `write(2)` ループを用い、出力完了後に変換前後の所有バッファを確実に解放します。
WASM のデフォルト動作では UTF-16 の表示結果バッファを解放するのみですが、オプトイン設定時には UTF-8 バッファを `tsuzuri_debug.write` へ同期転送して解放し、改行処理はホスト環境へ委ねます。
`llvm::EmitOptions` および `emit_with_options` が出力制御フラグを受け取り、従来の `emit`／`emit_target` はデフォルト値を指定するラッパーとして互換性を維持しています。

`llvm::emit_with_trap_info` はソースマップを明示的に受け取り、IR と `TrapSite` の対応情報を返します（デフォルトの API では計測処理を行いません）。
有効化されている場合にのみ、生成される関数呼び出しやトラップ命令に一時的なソースマーカーが付与され、`llvm_traps.rs` が生成 IR の関数、呼び出し命令、および引数リストを走査します。
内部関数および同梱ランタイムの末尾引数にはコンテキスト ID が追加され、間接関数呼び出しも同一の ABI に統一されます（公開エクスポート関数、C の main、および pthread コールバックは元の標準 ABI を維持します）。
標準のヘルパー関数群は呼び出し元のコンテキスト ID をそのまま伝播し、ユーザー定義関数では実際の式のソース位置（span）を採用します。失敗理由ごとのトラップ ID は、コンテキストと enum のオフセットから合成されます。
ソフトウェア浮動小数点演算の生成済み IR もこの計測対象に含まれますが、`numeric.c` および `numeric.ll` のデフォルト ABI を変更することはありません。
トラップ失敗ブロックのみがコールドなレポーター関数を呼び出し、ネイティブでは `write(2)`、WASM では内部グローバル変数とゲッター関数を使用します。正常実行パスで毎命令ごとにカレントサイトを更新・保存するような高コストなオーバーヘッドは存在しません。
計測変換によって副作用の属性が変化するヘルパーや呼び出しの attribute group は引き継がれません。また、一時的なソースマーカーは最終的な出力 IR から完全に除去されます。
ドライバーはサイドテーブルを一時ディレクトリに生成し、ソースファイルの保護状態を再確認したうえで配置し、成果物の公開処理が失敗した場合にはテーブルを安全にロールバックします。

Maybe 型の部分アクティブパターンおよび複数ケースを持つ全域アクティブパターンは、通常の関数呼び出しを `PatternStep::Bind` として保持し、既存の union case のタグ検査およびペイロード射影へと lowering されます。
複数ケースを持つ `def ... -> 'T = ラムダ式` の形式に対しては、パーサーが認識器専用の `UnionDecl` を自動合成します。チェッカーは本体内の case 名を通常のコンストラクタとして解決し、引数なし（nullary）の case も参照のたびに値を生成します。内部名にはユーザーが記述できない特殊文字を含め、型名や case 名の名前空間の衝突を防ぎます。
ペイロードの有無は本体内における case コンストラクタの直接適用から判定され、型情報は既存のチェッカーによる本体検査によって確定して関数の scheme へ反映されます。独立した型推論器や union ランタイムを新設することはありません。
自動合成された union 型は API ドキュメントやドキュメントシンボルの出力対象から除外され、API シグネチャ上には元の型引数 `'T` が表示されます。
マッチやガードに失敗した際の一時オブジェクトは既存のクリーンアップ機構によって解放され、追加の所有権ルールを導入することはありません。
名前テーブルは関数 ID と case 種別のみを保持し、ペイロード型は使用箇所ごとにフレッシュにインスタンス化したシグネチャから導出します。
複数ケースの `CoveragePat` は認識器、case インデックス、および case 総数を保持しますが、再評価を伴う現在のセマンティクスにおいては保守的に不透明（opaque）として扱い、フォールバック節の記述を要求します。

字句解析の `lex_all` は、不正な文字、不正な数値リテラル、未終端文字列を検出した場合でも可能な限り次のトークンから走査を再開し、未終端コメントは EOF まで消費して安全に停止します。
コードフォーマッタ用の `lex_with_trivia` は、同一のトークン列および span に加え、トークン間の空白、改行、およびコメント（trivia）の情報を完全に保持します。
`formatter::format_source` は AST 上の型、単項演算子、ブロック終端、分岐構文の構造位置を参照し、コードの意味を変えずに空白文字とインデントのみを美しく整えます。
フォーマット処理の前後で AST の構造を比較検証し（位置情報と深さを除く）、識別子の位置依存名は provenance 情報に基づいて正規化して等価性を確認します。
CLI はプロジェクト内の全入力ファイルのパースおよびフォーマット検証が正常に完了したことを確認したうえで、同一親ディレクトリ内の一時ファイルへの書き込み、flush、およびアトミックな rename を行います。
`--check` 指定時や変更のないファイルに対してはディスク書き込みを行わず、シンボリックリンクは拒否し、通常ファイルのパーミッション属性を厳密に保持します。
字句解析エラーが存在するファイルでは字句診断のみを即座に返し、欠落したトークンに起因する無用な構文エラーの連鎖を防止します。
`parse_with_source_all` はトップレベル宣言単位でのエラー回復機能を備え、消費済みトークンも含めて丸括弧 `()`、角括弧 `[]`、リスト括弧 `[| |]`、波括弧 `{}` のネスト深さを追跡し、ネスト深度 0 かつ 1 列目（行頭）に位置する `def`、`fn`、`record`、`union`、`type`、`class`、`instance`、`export`、`private`、`let` のキーワード位置でのみパーサーの再同期を行います（`and` は回復境界とはみなされません）。構文解析に失敗した宣言は定義シンボル集合へ登録されず、構文エラーが 1 件でも残存している場合は AST を後段へ渡しません。
全ソースファイルの構文診断を先行して収集することにより、構文が破綻したプロジェクトに対して無駄な型検査が実行されるのを防ぎます。

型検査は、名前空間、ファイル種別、型宣言およびメモリレイアウト、型クラスおよびインスタンスの各情報を段階的に収集し、構造的な不整合が検出された段階で安全に停止します。すべての関数名が登録された後に、各関数のシグネチャが個別に解決されます。
シグネチャの解決に失敗した関数は名前と ID を保持したまま、戻り値型が `Type::Error` に設定された毒されたシグネチャ（poisoned Scheme）として表現されます。
その関数への参照および依存する式には `TypedExprKind::Error` が安全に伝播され、poison に触れた関数本体における未確定制約、網羅性エラー、および二次的な派生診断の発生を確実に抑制します。
関数本体およびエントリーポイントの内部では、`Checker::expression` および `argument` の失敗に対するローカルな回復が行われ、ブロックの型注釈やサイズ検証の失敗も個別に回復されます。
エラー回復時には `RecoveryMark` を用いてスコープ長、ループ深度、およびコンピュテーション式深度が巻き戻されます。回復中の不正な期待型は「未知の文脈」として下位式へ渡され、兄弟式、match の各節、あるいは呼び出し不可能な対象に対する実引数についても可能な限り型検査を継続します。空コレクションリテラル、型注釈のないリテラル、ラムダ式の引数、ジェネリック引数など、文脈からしか型を決定できない要素のみを `Error` とし、依存する式に対する二次エラーは `finish_expression` および `contains_error` によって抑制されます。ブロック式は型のみを `Error` としつつ、構文的に正常な内部文の束縛情報を保持します。
大きな結果分岐の処理は非再帰的な `finish_recovery`、エラーハンドリングはコールドな `recover_expression` に分離されています。
二項演算子の検査も専用のディスパッチャに切り出されており、深いネストの式において汎用の大きな値検査フレームをスタック上に保持し続けないように最適化されています。リソース上限 `E1017` や診断収集上限に達した場合は、直ちにその関数本体の検査を打ち切ります。
型エラーが存在する場合にのみ、宣言 ID とインデックスを整合させた `recovery_module` が構築されます。検査不能な関数本体は閉じられた Error スタブへ、型確定に失敗した本体も Error 本体へと変換され、`ownership::check_recovered` が推論モードですべての関数を走査します。
Error スタブは借用（loan）を持たないため、この回復パスでは無効なメモリアクセスに関する診断のみを記録し、寿命不足エラーや Copy 制約は破棄されます。
所有権解析の `access` はルート変数ごとの最初の診断のみを記録し、診断済みルートに起因する寿命不足や移動後参照外しなどの二次エラーを抑制します。ネストしたブロック本体も回復済み診断を共有します。致命的な所有権エラーが検出された場合はその関数本体の解析を中断します。
有効なシグネチャを持つ他の関数本体およびエントリーポイントは独立して検査が継続されます。エラーが 1 件でも残っているプロジェクトでは、セマンティックインデックスの生成、特殊化、クロージャの lowering へ進むことはありません。
エラーのないプロジェクトでは、従来どおり Copy 制約の推論が行われ、特殊化完了後に `ownership::check_all` による最終的な所有権検査が実行されます。
`Type` や式に付与される `Error` は検査中のみの一時的な状態であり、正常に完了した `CheckedModule` には一切残存せず、LLVM コード生成の入口でも明示的にアサートされて排除されます。

**借用の表記:** キーワード記法（`ref`／`ref mut`／`deref`）と Rust 互換の記号記法（`&`／`&mut`／`*`）は、構文 AST 上の `Notation` の違いとしてのみ保持されます。型検査の段階で同一の型付き IR である `Borrow` および `Dereference` へと lowering されるため、所有権チェッカーおよび LLVM コード生成器が元の表記の差異を意識することはありません。
両記法から生成される LLVM IR が完全に一致することは、`tests/borrow_syntax.rs` によってネイティブおよび WASM の双方でテストされています。
キーワード `ref` は被演算子に対して期待型を与えずに型付けを行い、静的な型が既に参照型である場合は `Dereference` を自動挿入して 1 段階の貸し直し（再借用）を行います。
型変数の段階では貸し直しは行われません。型が未確定（`Infer`）の被演算子は `undecided_borrows` に記録され、関数の `finish` 処理でデフォルト型が適用された後に参照型へと解決された場合は `E1015` エラーとなります。推論結果に基づいて事後的に型付き木の形状を改変することはありません。
字句解析器では `ref` および `deref` を予約語として扱います。実引数位置における記号記法については、直前の空白の有無と被演算子との隣接関係に基づいて、前置の借用演算子（`f &x`）であるか二項ビット演算子（`a & b`、`a&b`）であるかを正確に判別します。

関数呼び出し、パイプライン演算子 `|>`、およびアクティブパターン認識器の実引数は、`Checker::argument` において期待型に合わせて借用、再借用、および参照外しが自動的に補完されます。
変数名、フィールドアクセス、配列添字アクセス、および明示的参照外しは、まず元の型で型検査された後に既存の `Borrow` や `Dereference` へと変換されます。
引数位置に置かれた関数呼び出しは `call_expression` で検査され、戻り値の参照の階層や可変性を保持した期待型を伝播させたうえで補完処理が行われます。
これにより、数値リテラルやジェネリックな式の文脈依存型推論が損なわれるのを防ぎ、引数式が二重に検査・評価されるオーバーヘッドを回避しています。
明示的な `Borrow` 式の型はそのまま検査され、通常の変数束縛、関数の戻り値、あるいは型が未確定の汎用値引数に対して推測によって勝手に借用を生成することはありません。
参照型であることが静的に判明している引数に対しては、排他参照を move することなく再借用を行い、非 Copy な参照先を値渡しとして不正にコピーできないように保護します。
所有権チェッカーおよび LLVM コード生成器は、明示記法と同一の構文木としてこれらを処理するため、競合検出、生存期間、クロージャ捕捉、および評価順序の規則が一貫して共有されます。引数検査の入口はコンパクトな振り分け処理とし、自動補完の作業バッファを再帰スタックフレーム上に保持し続けないように最適化されています。

**制御構文:** `while` ループ、整数範囲ループ、コレクション反復、および `match` 式は、それぞれ専用の型付き IR ノードとして表現されます。
ループ、分岐条件、およびパターンの型は型検査時に完全に確定され、LLVM コード生成フェーズにおいて型推論やイテレータの動的メソッド探索を行うことはありません。
タプルは構造的な値型として扱われ、型置換、メモリレイアウト計算、Copy／Capture／Send の制約判定、借用追跡、および clone／drop の処理へ再帰的に参加します。
型付き式の子要素走査ロジックが一元化されており、新しい制御構文の内部においても型置換、lambda lifting、使用回数の計数、および呼び出し特殊化の処理が漏れなく適用されるよう設計されています。

再帰呼出しの明示指定（`rec`）は構文 AST 上で保持されます。型検査後かつ単相化前の参照グラフに対して反復的な強連結成分（SCC）走査を実施し、自己再帰、相互再帰、関数値を介した再帰、アクティブパターン、ならびに型クラスインスタンスのメソッドや演算子の循環呼び出しに対して `rec` の明示を必須とします。
抽象型の型クラスメソッド呼び出しについては、該当クラスの実装全体を保守的に候補として参照グラフを構築します。
`def ... = ラムダ式` の省略記法は、従来のシグネチャと実装を結合する `Parser::define` を共有します。`def rec ... =` と型注釈付きの `and ... :: ... =` は同一の再帰グループを形成します。
宣言と実装を分離した `def rec`／`def and` においてもグループ構成は完全に一致し、非再帰関数の前方参照（forward reference）は従来どおり安全に維持されます。

ラムダ式の直後に記述された関数ガード節は既存の `guarded_definition` を共有し、`MatchOrigin::FunctionGuard` 属性を持つ通常の `match` 式へと変換されます。
後置 `where` 節はパーサーによって通常の `Block` 式内のローカル束縛へと変換され、結果式としてその match 式が配置されます。これに伴う新しい型付き IR やランタイム機構の追加はありません。
`where` 節内の初期化式は最初のガード条件の評価よりも前に記述順で 1 回ずつ評価され、型推論、スコープ、所有権、およびデストラクタの扱いは通常のローカル `let` と完全に共有されます。
`where` はガード節と同一のインデント行でのみ認識される文脈キーワードであり、字句解析器の予約語を無用に増やすことはありません。

パターンマッチは、左から右へと順序付けられた `Test`（判定）および `Bind`（束縛）のステップ列と、マッチ成功時の束縛元への参照へと展開されます。
OR パターン（`|`）では各分岐間で束縛変数名とその型を完全に一致させ、最初に成立した側でガード条件を 1 回だけ評価します。
ガードの評価が失敗した場合は直ちに次の match 節へと進み、同一の OR パターンの別の選択肢を再試行することはありません。
データ構造の長さ検査は各要素のロードよりも先行して行われ、共有参照を辿る場合も既存の借用規則が厳密に適用されます。
単一ケースのアクティブパターン（全域認識器）の結果は一時ローカル変数へと保存され、部分アクティブパターンは真偽値を返す `Test` として評価されます。
パターン検査の進行中は読み取り専用の別名（alias）として値を扱い、ガード条件が成立した後にのみ通常の所有ローカル変数へとコピーまたは move されます。
参照、コレクション要素、およびリストの tail からの束縛は `TypedMatchArm.borrowed` に記録され、非 Copy かつ借用を含まない具象型はマッチ成立後もコピーを行わずビュー（借用参照）のまま維持されます。OR パターンのいずれかの側がビューである場合は、両側の束縛がビュー形式に統一されます。
所有権モデルでは、このビュー自身を仮想的なメモリ領域のルートとし、元のメモリ領域に対して親の借用（loan）を保持した共有借用を維持します。
これにより、ビューから派生した借用が生存している間も元のデータ構造が安全に保護され、ビューからの借用を match 節の外へ返却することは禁止されます。
抽象型において値の複製が必要となるアクセスでは、`infer_copy` によって Copy 制約が推論され、具体化された Copy 値は従来どおりマッチ成立時に安全に複製されます。
LLVM はビューをガード評価時と同一の射影ポインタのまま読み出し、move や drop は行わず、元のデータ所有者のみが最終的な解放を担当します。
どの Test やガード条件で不成立となった場合でも、それまでにアクティブパターン認識器が生成した一時的な所有値は確実に解放されます。
一時領域のスロットはエントリーブロックに配置され、別の OR 経路で使用されなかったスロットはゼロクリアしておくことで、安全なデストラクタ実行を保証します。
すべての節が不成立となった場合は `llvm.trap` または `unreachable` 命令となり、成功を仮定した不正な既定値が生成されることはありません。
OR パターンの展開は最大 1024 通りまでに制限され、構文および展開後のネスト深度にも既存の最大 128 の上限が適用されます。

網羅性解析（`exhaustiveness.rs`）は、`control::pattern_alternatives` が lowering と同時に名前解決および認識器解決から生成する型付きの `CoveragePat` を利用し、構文 AST を再パース・再解決することはありません。
解析対象は `MatchOrigin::{Explicit, FunctionGuard}` を持つ明示的な match 式に限定され、ラムダ式の引数パターン、ソースコード上の `for` ループ、およびコンピュテーション式内の分解については静的網羅性検査を行わず、実行時のトラップ挙動を維持します。
定数パターンのキーはデフォルト型確定後の具体的な型に依存するため、網羅性検査は関数ごとの `finish` 処理の完了後に実施されます。
定数キーは、解決された型と実行時の等価性規則に合致した正規形（整数のビット表現、浮動小数点数の $\pm 0$ の同一視と NaN の空パターン化、decimal のコホート正規化）へと変換されます。
パターンの有用性（usefulness）は行列の特殊化アルゴリズムによって判定され、`Rc` による永続的な行構造を反復的な深さ優先探索（DFS）で走査するため、ネイティブのコールスタック消費量は入力の深さに依存しません。
完全なシグネチャ（網羅可能な有限集合）として扱われるのは、`bool`、`unit`、タプル、レコード、union の全 case、およびリストの空リスト／cons であり、整数、浮動小数点数、decimal、文字列の定数、ならびに配列の長さは常に不完全なシグネチャとみなされてデフォルト行へと進みます。
部分アクティブパターン、および戻り値パターンが網羅的でない全域アクティブパターンは、網羅性の行には何も寄与せず、到達可能性の問い合わせにおいては任意の値にマッチし得るものとして扱われます。
すべての列がワイルドカードである行を含む部分問題に到達した時点で、その領域は網羅済みとして探索を打ち切ります。
1 つの節の正規化が 1024 通りを超えるか、探索の総作業量が $2^{24}$ ステップを超過した場合は `E1017` リソース上限エラーとなります。
非網羅なパターンは最初に到達した match 式の位置で `E1021` エラーとなり、到達不能な節が存在する場合はそのパターンの位置で警告 `W1003` が報告されます。
網羅性を静的に保証したコードであっても、LLVM の失敗ブロックにおける `llvm.trap` や `switch` の既定節は防御的プログラミングとして常に残されます。
警告は `CheckedModule.warnings` にソース位置順で整列して保持され、単相化やクロージャ変換を経た後も失われることなく、CLI が `Project::source_for` を通じて該当ファイル上の正確な位置に表示します。

ループ構文における所有権の反復解析は、ループ入口、継続条件、ループ本体、およびバックエッジにおける move と loan の合流を、不動点（fixed point）に到達するまで追跡・検査します。
`break` および `continue` は unit 型を持つ専用の IR ノードです。型検査における通常のループ文脈は、ラムダ式、タスク、およびコンピュテーション式ビルダーの境界で分断され、ビルダー展開後には深さを増加させない内部境界 `ComputationBoundary` を配置することで、外側のループへの不正な大域ジャンプを防止します。
所有権解析は、正常継続、break、continue の 3 つの状態を分離して管理し、break と continue の状態からは一時ローカル変数を除去したうえで借用関係を検証します。
正常継続と continue の状態のみを不動点解析のバックエッジへフィードバックし、条件式が false となった状態と break の状態をループ出口で合流させます。各反復ごとに辺（エッジ）情報の収集をやり直します。
収集するエッジ数はループあたり最大 4096 件、不動点解析の反復回数は既存の上限を維持し、超過時は `E1017` となります。
LLVM はループごとの分岐先、ローカル変数、および評価済み一時値のスタック深度を保持し、ジャンプ実行前に内側スコープの所有値を確実に解放します。
部分的に初期化された配列やリストの解放処理では、初期化が完了している接頭部のみを解放し、未初期化領域を読み出すことはありません。
所有値を関数呼び出しや構造体格納へ渡した時点で一時値の追跡対象から除外されます。ジャンプ命令の後には到達不能ブロックを開始し、適切な phi ノードの生成を維持します。
新しいループ反復で使用されるローカル変数は反復ごとに再初期化され、ループ外へ漏洩した参照が反復ローカル変数を指している場合は借用チェッカーが拒否します。
状態の比較においては、借用の生成 ID ではなく実際の参照先アドレスと可変性フラグを比較し、上限を超える複雑な解析は `E1017` として安全に打ち切ります。
ループ内部での構文上 1 回限りの変数使用を「実行時にも 1 回しか通らない」と過信して、所有権をループ外から奪い取ってはなりません。
反復対象となるコレクションはループ開始時に 1 回だけ評価されて読み取り借用され、配列や文字列は連続メモリアドレスの走査、リストはポインタを辿る $O(n)$ の走査となります。
`string` は 16-bit の UTF-16 コード単位、`utf8string` は 8-bit の UTF-8 バイトをメモリから直接ロードします。
添字をインクリメントしながらリストの先頭から繰り返し探索したり、反復処理のためにコレクション全体をディープコピーしたりすることはありません。

狭い整数型の単位刻みループ（ステップ 1 のループ）は内部的に 64-bit（`i64`）の誘導変数（induction variable）へと拡張され、終端の次の値もオーバーフローすることなく表現できるようにします。
ループ本体に入った値が元のビット幅に収まっているという事実のみを `llvm.assume` を通じてオプティマイザへ伝え、ユーザーの算術式に安易にオーバーフロー抑止フラグ（`nsw`／`nuw`）を付与することはありません。
64-bit および 128-bit の単位刻みループは終点値を処理した時点で確実に停止し、任意のステップ幅を持つループはオーバーフロー検出 intrinsic と範囲比較を組み合わせて停止判定を行います。
密な整数定数の match 結果に対しては、最大 256 要素かつ密度 1/2 以上の条件を満たす場合に静的ジャンプテーブルを生成し、それ以外の整数定数選択は `switch` 命令へ、複雑な構造・ガード・アクティブパターンを含む分岐は順序付きの条件分岐へと lowering されます。ジャンプテーブルの範囲外の値も明示的な既定節（default）へ確実に分岐します。
代入が 1 箇所のみの小さな整数リダクション（集約）ループには LLVM のループ展開（unroll）ヒントを付与しますが、固定の展開数、特定 ISA、あるいは演算の再結合を強制することはせず、浮動小数点のリダクションやループ依存性を持つ整数の攪拌処理（ミキサー）には適用しません。
ループ制御用のメタデータは、同梱の数値ランタイムが使用するメタデータ ID 範囲と完全に分離されており、すべての LLVM IR を決定的に生成します。

**コンピュテーション式:** `.tc` ファイル内に定義されたすべての関数名を順序付き集合として収集し、そのファイル名をビルダー名として扱います。
`match!` は `Bind` と通常の `match` 式の組み合わせへと展開され、`and!` は各入力ソースを順序付きの一時 `let` 変数に保持したうえで、`MergeSources` の左結合と `Bind` へと展開されます。
構文的に末尾の 2 文が `let!` + `return`、または 2 要素の `and!` + `return` である場合に限り、ビルダー内に定義が存在すれば最適化された `BindReturn` または `Bind2` が優先選択されます。`Bind2` の継続関数は 2 つの引数を同時に束縛し、`mut` や型注釈の情報も適切に保持します。
これらの解析および展開処理は専用のヘルパー関数に分離されており、構文解析の最大深度 128 および 2 MiB のテストスレッドスタックの安全性を維持します。新しい `TypedExpr` バリアントや専用ランタイムを追加することはありません。
利用側の `Builder { ... }` 構文は元のソース位置（Span）を保持した専用の構文 AST として表現され、各関数およびエントリーポイントの型検査に先立って、
`Bind`、`Return`、`ReturnFrom`、`Yield`、`YieldFrom`、`Zero`、`Combine`、`For`、`While`
に対する通常の関数呼び出しおよび匿名関数（ラムダ式）へと構文レベルで展開されます（`Delay` および `Run` はビルダー内に定義が存在する場合にのみ呼び出されます）。
継続関数は残りの文全体を内包し、`let!` の型注釈や `do!` の unit 制約は自動生成されたローカル束縛を通じて静的に検証されます。
型注釈のない `let!` は継続関数の引数へ直接束縛されるため、余分な別名変数の作成による配列やクロージャ環境の無駄なコピーを発生させません。
単純な `let` 束縛や unit を返す式は同一ブロック内にまとめられ、平坦な束縛列とすることで不要な関数の再帰呼び出しを排除します。
展開は最も内側の式から順に行われ、外側の親式を含めたネスト深度が再計算されます。個々の式展開のみを局所的に検査して入れ子の遅延処理や継続の合計深度を見落とすと、型検査時にスタックオーバーフローを引き起こす危険があるため、構文木全体で厳密に深度上限を管理します。
型検査および所有権検査においては、関数呼び出し、匿名関数、およびブロックの処理を、巨大な式や演算の分岐処理から分離し、深度制限内の深い継続を検査する場合でも各再帰呼び出しフレームが大きなスタックメモリを消費しないように最適化されています。
自動生成される変数名にはユーザーが記述できない `$` プレフィックスが付与され、操作関数の呼び出しはローカル変数を経由しない完全修飾参照として展開されるため、ユーザー定義の `Bind` 関数やビルダーと同名のローカル変数によって展開先が乗っ取られる心配はありません。

型付き IR 以降のコンパイラフェーズには、カスタムビルダー専用のデータ構造、専用命令、あるいは専用ランタイムは一切存在しません。
通常の型クラス制約、特殊化、Capture、関数環境の複製、生存期間、および move／drop の意味論がそのまま適用されます。
必要な操作がビルダーに定義されていない場合は `E1018` エラーを報告し、型や所有権の不整合は既存の診断機構で厳格に拒否され、成功を装った既定実装への暗黙的なフォールバックは行いません。
`Delay` が定義されていない場合は `Combine` の両引数は先行評価（正格評価）され、`Delay` が存在する場合は第 2 引数の本体が遅延関数へと渡されます。
`While` は bool を返す unit 関数と、明示的な `Delay` の結果を受け取ります。
空の `Name {}` 構文は、収集済みビルダーに該当名が存在する場合はビルダー式を優先し、存在しない場合は既存の空レコードとして解釈されます。
組み込みの `task` ビルダーにおけるコールド実行、非 Copy、Send 制約、および 1 回消費の最適化経路は独立して維持されます。
カスタム展開の都合によって、通常の関数値に 1 回実行タスクや排他参照を不正に捕捉させるような特例を導入することはありません。

ビルダー名を明示しない暗黙のコンピュテーション式本体は、同一の AST ノードにユーザーが記述できない内部合成名を付与して保持され、型検査の段階で段階的にビルダーが特定・展開されます。
戻り値型、または最初の `bind` 入力型と各ビルダーの公開シグネチャを照合して候補を絞り込み、候補の照合には既存の型推論器（`Inference`）の独立したクローンを使用します。
候補照合の段階で式本体を無駄に再型検査することはせず、実際に検査を通過した右辺式は自動生成されたローカル変数へ 1 回だけ保持されます。複数のビルダーが適合して曖昧性が生じた場合は `E1018` エラーとなります。
通常のプレフィックス式および型の決定に寄与した最初の右辺式も、選択された `Delay` または `Task` のクロージャ内部へと安全に配置されます。
同一ビルダー内では従来の操作関数が呼び出され、異なるビルダー同士の組み合わせは外側の `Return` による入れ子構造へ、`Using` が存在する場合は通常の高階関数による合成へと展開されます。
`Using` へ渡されるソースは、異種ビルダーの `Bind`／`Return`／`Delay`／`Run` を共通の lowering ロジックで組み立て、IO においてはそのコールバックから得られた IO アクションを実行時に消費します。
クロージャによる変数の捕捉、Capture／Send 制約、および環境サイズ検査は通常の `checked_lambda` と共有されます。専用の `TypedExpr` やランタイムインタープリタを新設することはありません。
巨大な AST 構築ヘルパーは再帰的な型検査処理から分離されており、暗黙の展開に対しても既存のネスト深度 128 の上限が厳格に適用されます。
なお、シグネチャを持たない `fn main` は他の通常関数と同様に `E0002` エラーとなり、診断メッセージにおいて 2 つの正規なエントリーシグネチャが案内されます。
検証は `tests/computations.rs`、`tests/computations.mjs`、および `tests/io.mjs` により、型整合性、曖昧性検出、短絡評価、コールド実行、所有値の安全な解放、ならびにネイティブおよび WASM の `-O0`／`-O3` で行われています。

**多相性:** 明示的なシグネチャに現れる型変数はリジッド（rigid：硬直型変数）として固定され、各関数の参照箇所で導入される推論変数のみが単一化（unification）の対象となります。
occurs check（出現検査）および型のネスト深度・構成要素数の上限が適用され、型を確定できない関数値はコンパイルエラーとして拒否されます。
型クラスの要件は演算子、メソッド呼び出し、および所有権の制約から収集され、呼び出しグラフ上の不動点に達するまで伝播されます。
型クラス定義とインスタンスヘッドの単一化可能性を検査して重複（overlap）を拒否し、型別名についても正規化した後に比較を行います。
インスタンス宣言の本体コードは、プロジェクト内で未使用であっても通常の関数とまったく同一の型検査を受けます。
ジェネリック関数の本体も宣言時に抽象型のまま型検査を受け、単相化（特殊化）の段階で具体的な型引数に基づく型、所有権、生存期間、メモリレイアウト、およびリテラル範囲の条件が再検証されます。C++ テンプレートのように呼び出し時に初めて構文検査を行う方式とは根本的に異なります。
特殊化は `(宣言 ID, 型引数列)` をキーとするキャッシュと作業キューによって管理され、再帰関数に対しても同一の ID が割り当てられます。
型変数、未解決のメソッド参照、および未エンコードのリテラルが LLVM コード生成へ渡されることはありません。
仮想関数テーブルや動的ディスパッチ辞書、あるいは汎用値の boxing は不要であり、ゼロコスト抽象化を実現しています（vtable を使うのは利用者が明示した `dyn` 型だけです。後述の「dyn 値」）。
なお、古い宣言構文は互換性テストのために残されていますが、新しいサンプルコードでは分離シグネチャと空白区切りの引数適用を使用します。

`ClassDecl` は、既存のシグネチャ列にスーパークラス（superclass）とデフォルトメソッド定義列を保持し、名前によって関連付けられます。
`Classes.instances` は、ヘッド型、コンテキスト制約、メソッド ID、および型引数を持つ単一のテンプレート集合として管理されます。
具象インスタンス専用の独立キャッシュは作らず、既存の `Specializer` が持つ `(関数 ID, 型引数列)` キャッシュを活用します。
インスタンスヘッドの型変数のみをフレッシュな推論変数に置き換え、要求側の抽象型変数は rigid に保った状態でパターン照合を行います。
`normalize` は、スーパークラス、条件付きインスタンス、および構造比較の要素要件を反復的に展開し、残余の制約を不動点解析へと引き渡します。
処理中（active）と完了（completed）の状態を区別して循環参照を誤って成功とみなさないようにし、重複する制約のみを除去します。再帰深度は最大 64、個別制約数は最大 128、インスタンスの重複判定は最大 1024 組を上限とします。
デフォルトメソッドに対しては、宣言モジュール内に汎用の `$instance.default` 関数が 1 つ自動合成され、未使用の場合でも通常の型検査および所有権検査が行われます。
メソッド本体の実装から生じた追加の制約は、class または instance の明示的な context 制約によって証明可能でなければなりません。
抽象段階における保守的な再帰判定に加え、具体化後にも同一の強連結成分（SCC）解析を適用し、型サイズが縮小する健全なテンプレート展開と、無限に型が膨張する実際の循環呼び出しとを正しく区別します。

条件付きの `Eq` および `Ord` インスタンスは、通常の単相化済み関数の内部に `StructuralCompare` ノードを保持します。
子要素には左右の共有借用参照と各要素型の具体的な比較メソッド参照を保持し、通常の到達可能性解析および型走査の仕組みを共有します。
LLVM では、配列の添字アクセス、リスト走査用の 2 本の phi ノード、およびタプルのフィールド GEP 命令を用いてコード生成を行い、最初の不一致を検出した時点で短絡評価して高速に終了します。
各要素の比較には既存の借用 ABI が用いられ、部分適用された比較メソッドに対しても残りの引数が順次適用されます。型辞書、汎用 boxing、要素の不要なコピーは一切発生しません。

**導出（deriving）:**
`derive::instances` は、record や union の宣言から通常の `InstanceDecl`、式、およびパターン AST を自動合成し、手書きのインスタンスと完全に同一のコヒーレンス検査、型検査、所有権検査、および特殊化パイプラインを通過させます。
合成された識別子や内部クラス参照は provenance（由来情報）によって区別され、同名のユーザー定義モジュールへと誤解決されることはありません。診断メッセージは元の `deriving` 記述の span を正確に指し示します。
自動導出された構造的要件に限定して同一クラスの再帰呼び出しを許容し、`Default` インスタンスの循環定義は厳格に拒否します（一般的な循環インスタンスは引き続き `E1017` エラーとなります）。
`Hash` のプリミティブ演算では、ビット幅に応じた整数の load、shift、xor、および wrapping multiply を行い、decimal 型では `numeric.c` の decode/encode 処理を共有します。
合成されたコレクションヘルパー内では `StructuralHash` および `StructuralDisplay` を使用し、子要素の具体的なメソッドを通常の到達可能性解析に含めます。
値の文字列表現では内部の `DisplayQuoted` 組み込み関数を具象型へと解決し、`runtime/display.ll` が UTF-16 の引用符処理と文字列片の一括結合を担当します。
`Encode`／`Decode` の導出（D08）は、レコードでは `$object{i}`（encode）と `$field{i}`／`$failed{i}`／`$error{i}`（decode）の束縛の連鎖、union では 1 段の `match` を合成します。std の補助関数（`Json.encode_field`、`Json.decode_field`、`Json.case_index` など）と `Result.is_error` などは `QualifiedFunction` のモジュールキー（`Json.begin_object`）で、`Result.Ok`・`Maybe.None` などの case はキーパス（`::Result.Result`）で参照するので、利用者の名前空間や同名の宣言に解決されません。深さはフィールド数・case 数に比例しないため、128 フィールドや 128 case も深さ 128／4096 節点の上限に収まります。フィールドと case の `@json "名前"`（`Parameter::json`・`UnionCaseDecl::json` の `JsonName`）はキーとタグの文字列だけを変え、`derive::json_names` が `Encode`／`Decode` の導出の無い型での使用と名前の重複を `E1025` にします。formatter は属性のトークンをそのまま並べ（span は fingerprint から除く）、docgen は属性ごと表示します。成分の instance の欠落は std の補助関数を経由して単相化の制約伝播で見つかるため、`specialize` の正規化も `derived_error` で `E1025` に読み替えます。
インスタンスの重複判定（`Classes::instances`）は、ヘッドをクラスと最外の型構築子（スカラーの種類と幅、record／union の id、配列・リスト・`Vec`・`Task`、タプルの長さ。型変数や高階のヘッドは `None`）で索引し、同じ構築子か `None` のヘッドとだけ単一化します。1024 組の上限は単一化した組だけを数えるので、std の `Json` が多数のインスタンスを持っても、別々の record に導出したインスタンスが何百あっても予算を消費しません。
Hash の canonical stream 仕様および文字型ごとの引用規則の詳細は言語仕様を参照してください。なお、`numeric.ll` は生成スクリプトから自動生成されるため、手動で直接編集してはなりません。

**文字列補間と書式指定:** `$"a{x}b"` および `u8$"..."` 形式のリテラルは、字句解析器によって `InterpolationStart`、`InterpolationMiddle`、`InterpolationEnd` の各トークンに分割され、構文解析器によって `ExprKind::Interpolated`（固定文字列片と `InterpolationHole { value, spec }` の列）へと変換されます。字句解析器は開かれた埋め込み穴を `holes` スタック（`OpenHole { utf8, depth, start }`）で管理し、括弧のネスト深度が 0 の位置にある `}` または `:` によって穴を閉じます。埋め込み穴は同一行内に収める必要があり、穴の内部にコメントを記述することはできません。埋め込み穴を含まない文字列リテラルは通常の文字列トークンとして維持されます。リソース制限として、1 つのリテラルあたり最大 1024 個の穴、幅と精度は最大 4096、ネスト深度はパーサーの標準上限が適用されます。
型検査（`interpolation`／`interpolation_hole`）は `TypedExprKind::Interpolated(TypedInterpolation { texts, holes })` を構築し、型はリテラルのプレフィックスに応じて `string` または `utf8string` となります。`TypedHole.operand` は常に共有借用 `ref U` となります。評価場所（変数名、フィールド、添字、参照外し）は `ref` 引数と同様に借用され、既存の参照はそのまま再利用され、それ以外の評価結果は `BorrowOperand` による一時変数として確保された後に文字列結合完了時点で安全に drop されます（引数のない関数呼び出しや単独の union case のように値を生成する式も一時値として扱われます）。穴の式は左から右へと順次 1 回ずつ評価され、値の所有権を消費することはありません。適用されるメソッドは原則として `Display.display`（カスタム書式の場合は `Format.format`）であり、リテラル自身と同一の文字列型の穴や、符号・精度・型指定を持つ数値の穴に対してはメソッド呼び出しを行いません。
LLVM コード生成（`llvm_display.rs`）では、すべての穴を左から右へ評価して `Piece`（データポインタ、長さ、所有者、ASCII 判定、パディング情報）を構築し、リテラルの固定長、各穴の展開長、およびフィル文字の必要数を合算して、最終結果を `@tz.string.allocate` または `@tz.utf8string.allocate` によって 1 回のメモリアロケーションで確保します。固定文字列片はメモリコピーで書き込まれ、穴の内容は各値の表示バッファから直接コピーされます（リテラルと同一型の文字列の穴は Display を経由せず元のバッファを直接読み込み、数値の ASCII 出力は UTF-16 結果の場合に `@tz.format.widen` で拡張しながらコピーします）。結合完了後、すべての一時値が解放されます。`u8$` リテラルにおいては、Display が生成した UTF-16 の結果を `@tz.utf8string.from_string` で UTF-8 へ変換し（孤立サロゲートを検出した場合はトラップ）、変換元のバッファを解放します。
数値の書式指定処理は、ホスト環境の `printf` を使用せず、`src/runtime/numeric.c` の `tz_soft_format_spec` が多倍長の係数から直接最近接偶数丸めを行って ASCII 文字列を出力します（`numeric.ll` は `python3 src/runtime/generate.py`、`math.ll` は `python3 src/runtime/generate_math.py` で再生成します）。`flags` は bit 0 で `+` 符号を、bit 4 以降で出力スタイル（0: デフォルト、1: `f`、2: `e`、3: `x`、4: `X`、5: `o`、6: `b`）を表現します。作業バッファには、`format_capacity` が型と精度から算出するサイズ（512 バイト以下）のエントリー `alloca`、または `@tz.alloc` で確保されたヒープ領域が使用されます。パディング用の補助関数群は `runtime/format.ll` に配置され（スカラー数計数の `@tz.format.scalars.utf16`／`utf8`、フィル書き込みの `@tz.format.fill.u16`／`u8`、ASCII 拡張の `@tz.format.widen`）、前後のフィル数は LLVM が穴ごとに正確に計算します。幅のカウントは Unicode スカラー数に基づいて行われます（サロゲートペアは 1、孤立サロゲートも 1、数値の ASCII は文字数そのもの）。余白の不足分は、デフォルトで数値は左揃え（右パディング）・その他は右揃え（左パディング）となり、中央揃え `^` は不足分の半分（端数切り捨て）を前方に配置します。

**Format クラス（D07 Phase 2）:** `Format<'a>` は組み込み型クラスの 27 番目として定義されており、メソッド `format :: ref 'a -> ref string -> string` はコンパイラ組み込みの実装（`Operation`）を持たず、ユーザー定義の `instance` 実装に委ねられます。インスタンスのヘッド型は、`validate_format_instance` によって現在のプログラムで宣言された record または union（型引数は自由）に限定され、それ以外の型に対する実装は `E1016` エラーとなります（到達不可能な無効インスタンスの作成を防ぐためであり、`validate_drop_instance` と同一フェーズで検査されます）。クラス名 `Format`（ユーザーによる `record Format` の定義は `E1001`）およびモジュール名 `Format`（`E1011`）は予約名です。`TypedHole.custom` は、書式指定が存在し、穴の型が record または union であり、かつ指定に符号・精度・型が含まれるか `has_format_instance` が真である場合にセットされます。このとき `method` は `Format.format` に設定され、適合するインスタンスが存在しなければ `E1005`（`no instance for Format<T>`）エラーとなります。条件付きインスタンス（`instance Format<'a> => Format<Box<'a>>`）もサポートされます。カスタム指定でない穴の書式文字列は、`check_format_specs` が推論後の型に対して妥当性を検証し（`+` は数値型、精度は float および decimal、`x X o b` は整数型）、不整合がある場合や型が未確定のまま残った場合は `E1003` となります。`Display` のみを持つ record や union に対して幅とアライメントのみを指定した場合は、従来どおり Display の出力結果に対してパディングが行われます。
`format_piece` は、借用した対象値と、検証済みの書式指定を正規の文字列表現（デフォルトのフィルや省略要素を除いた形式、例: `*>+8.2f`）にした文字列定数を、スタック上の `%tz.string` 記述子経由でインスタンスへ渡し、返却された文字列をそのまま穴の展開結果として使用します。コンパイラ側で追加のパディングや後処理を行うことはなく、幅や配置の適用はインスタンス実装側の責任となります。`std/Format.tz` に用意されている `Format.parse :: ref string -> Maybe<Format.Spec>`（字句解析器と同一の構文規則に従い、不正な文字列、先頭が `0` の幅や精度、4096 超過、禁止文字などを `None` として拒否）および `Format.pad :: ref Format.Spec -> string -> string`（スカラー数に基づくパディング）は、このための標準ライブラリ実装であり、コンパイラコアから直接参照されることはありません。なお、実行時に動的構築された書式文字列、`deriving (Format)`、`#` や `0` フラグ、ロケール依存処理、および書記素クラスタ（grapheme cluster）幅の考慮は現時点で未実装です。

**文字列補間の編集支援と検証:** VS Code 向け TextMate 文法では、`string.interpolated.tsuzuri`、`meta.embedded.interpolation.tsuzuri`、および `constant.other.format-spec.tsuzuri` の各スコープが付与されます（`vsc/syntaxes/tsuzuri.tmLanguage.json`）。LSP は穴の内部の式を型および定義のインデックスに含め、固定文字列片や書式指定の位置では不要なコード補完を抑止します。フォーマッタは穴の式の前後に余分な空白を挿入しません。
検証は `cargo test --locked --test string_interpolation` および `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri string_interpolation` で実施されます。後者は書式仕様、パディング、および `Format` インスタンスの動作を、独立した JavaScript 参照実装およびネイティブ／WASM の `-O0`／`-O3` 出力と照合します。`tests/lsp.rs` および `vsc/src/test/grammar.test.ts` がエディタ連携機能を担保しています。

**関数シグネチャの制約行:** インデントされた `@'T : Class, #function` 形式の記述は、`ConstraintExpr` において型クラス名と関数要件名に明確に区別されて保持されます。型クラス要件は従来の `Constraint` へと lowering され、インラインの `Class<'T>` 記法や前置制約とも共存します。
関数要件は `MemberConstraint` として対象型、関数名、参照元モジュール、および呼び出し時の関数型を正確に保持します。`'T.function` の構文ノードは識別子を内部でボックス化し、専用パーサーへ分離することで、構文解析時のスタック消費量を抑制しています。
関数本体の収集後、`solve_members` が関数要件をコールグラフ上で不動点に達するまで伝播させ、対象型が具体的なレコードや union に確定した要件を `Names::member_function` によって定義元モジュールから解決します。
要求される関数型と選択されたシグネチャとを単一化し、呼び出し元の戻り値型を確定させた後に `finish` および網羅性検査を行います。
数値型のデフォルト型決定は、確定可能な関数要件をすべて解決した後に適用され、その後に残った要件に対して再度解決を試みます。
関数要件は重複を除いて関数あたり最大 128 件までに制限され、伝播される型情報にも既存のネスト深度および要素数の上限が適用されます。
抽象メンバー参照に対する再帰呼び出し検査では、同一名の可視なモジュール関数を保守的な候補として扱います。
単相化フェーズにおいて可視性、存在性、および関数型が再確認され、通常の関数参照へ変換された後にクロージャ捕捉、所有権検査、および closure lowering が適用されます。
`private` の可視性検査は常に参照元モジュールのコンテキストを維持し、呼び出し元や具体型のモジュールへと権限が不当に移転することはありません。
型変数のまま未解決の関数参照が LLVM コード生成へ渡されることはなく、専用の動的ランタイムや実行時探索機構を追加することもありません。

**整数 intrinsic:** `Int.*` 関連の関数は通常の多相組み込み関数として定義され、各整数ビット幅に応じた LLVM の直接命令および intrinsic へと lowering されます。
ビット数カウント処理におけるゼロ入力時の未定義動作フラグは false に設定され、ビット回転数はビット幅に合わせて明示的にマスク処理されます。チェック付き演算（`checked_*`）は既存の `Maybe` union の構築処理を共有します。
`i128` の checked および saturating 乗算は、64-bit limb の部分積とキャリーフラグを解析して上位 128-bit のオーバーフローを厳密に検出し、符号付き演算では絶対値の許容限界も正確に判定します。これにより、未同梱の外部ランタイムライブラリ（`__muloti4` など）に依存することなく、自己完結したコード生成を実現しています。
整数のべき乗はバイナリ累乗法（二乗法）で実装され、最後の指数ビットの処理後に不要な二乗算を行わないように最適化されています。

**消費型コレクション更新:** `Array.set`、`update`、`swap`、ならびに `List.cons`、`tail` は、通常の多相 `BuiltinInstance` として提供されます。
既存の組み込みラッパーおよび emitter の境界検査、drop、関数適用の仕組みを共有し、個別の専用フック関数は持ちません。
呼び出し側の Copy／move およびスタックフレームからのヒープ移送が完了した所有バッファに対してインプレース更新を行うため、組み込み関数内部でコレクション全体を不必要に再複製することはありません。
`update` における更新前の旧要素および関数のクロージャ環境は、所有権を持つクロージャ呼び出しへと安全に引き渡されるため、適用完了後に二重解放されることはありません。
`tail` は後続のリンクを読み出す前にリストの空判定を行い、先頭ノードのみを安全に解放します。確保・解放の回数は、E2E テストにおいて単一使用、旧バッファの再利用、およびスタック移送の各ケースで厳密に検証されています。

**遅延反復（Seq）:** `Seq` は標準ライブラリの不透明な record 型として定義され、常に非 Copy（所有型）となります。`head` は `Maybe` の要素値、`step` は `Maybe` のクロージャを保持し、空シーケンスや 1 要素シーケンス（once）の生成ではクロージャ環境の動的確保を必要としません。
`Seq.next` のみが型付きの組み込み関数として内部表現を直接検証し、`head` を取り出すか、`step` クロージャを所有モードで 1 回だけ呼び出します。反復のたびにクロージャ環境全体をディープコピーすることはありません。
`defer`、`unfold`、`map`、`filter`、`to_array` は通常の std ソースとして記述されており、クロージャ捕捉および借用の伝播ルールを共有します。`filter` は述語関数に要素を共有借用として渡すことで、フィルタリング中の要素の喪失を防止します。
`for ... in seq` 構文は、型検査時に既存の Block、While、Match、Assign、Break を組み合わせた制御フローへと展開され、反復ごとに次の状態を復元してからユーザーのパターンとループ本体を型検査します。
展開時には完全修飾された組み込み関数参照が用いられるため、ローカルスコープに `Seq` という名前の変数が存在しても展開先が狂うことはありません。既存の不動点所有権解析およびループクリーンアップ機構を共有し、通常のコレクションに対する直接 `for` ループの挙動には干渉しません。
`Array`、`Vec`、`Map`、`Set` からのシーケンス生成は共有参照とインデックスを用いてステップ関数を構築し、`List` からの生成では `fold_ref` によって参照の `Vec` を 1 回構築したうえで所有状態としてシーケンスへ移送します。
検証は `cargo test --locked --test iteration_protocol` および `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri iteration_protocol` で行われています。

**順序付きコンテナ:** `Map`、`Map.Entry`、および `Set` は、標準ライブラリの不透明な record 型として登録され、構築、フィールドアクセス、パターンマッチ、および更新操作が定義モジュール内に厳密に限定されます。
型名は通常のジェネリックレコードであり、内部的には `Map` が `Vec<Entry<K, V>>` を、`Set` が `Vec<K>` を保持します。`Vec` が持つ非 Copy 性、メモリ確保、および clone／drop の仕組みをそのまま共有し、ランタイムに新しい特殊な内部表現を追加することはありません。
二分探索（`lower_bound`）によって借用比較を行い、コレクションの更新は `Vec.push`、`pop`、`swap` を組み合わせた所有値のインプレース移動によって行われます。重複キーの挿入時には、既存のキーインスタンスを維持したまま値（value）のみを置換します。
`Set.union` は末尾からの要素比較と pop を利用して逆順の一時コレクションを作成した後に一括反転し、`intersect` および `difference` は 2 本のインデックスポインタを進めながら効率的に要素を選択します。
不透明レコードの内部には共有参照を格納できますが、排他参照（`ref mut`）の格納は拒否され、通常のレコードを経由して隠蔽格納しようとする試みも型検査によって確実に検出されます。
NLL（非字句的生存期間）における借用情報の解放処理では、生存している借用の参照先と親要素を正しく追跡し、読み出しに必要となる元の所有者情報を維持します。
検証は `cargo test --locked --test map_set` および `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri map_set` で行われています。

**ハッシュコンテナ:** `HashMap`、`HashMap.Entry`、および `HashSet` も `Map` と同様に不透明な標準 record として定義されており、専用の `Type` バリアント、特別な組み込み命令、あるいは C 言語ランタイム関数を必要としません（`stdlib.rs` における予約型名と不透明レコード一覧への登録のみで完結しています）。実装は `std/HashMap.tz` および `std/HashSet.tz` の純粋な Tsuzuri ソースコードであり、`HashSet<'key>` は内部に `HashMap<'key, unit>` を 1 つ保持する構成をとります。
`HashMap<'key, 'value>` は、挿入順に密に格納されたエントリー配列 `entries: Vec<Entry>`（ハッシュ値・キー・値）、エントリーのインデックスまたは `-1` を保持するオープンアドレス法の索引テーブル `slots: Vec<i64>`、ならびに非公開フィールド `key0`、`key1`、`keyed` から構成されます。
索引テーブルは線形探査（linear probing）を採用し、サイズは常に 2 の冪乗（最小 8）、最大負荷率は 1/2 に制御され、`2 * (count + 1)` がテーブルサイズを超過した時点で容量を倍加して再ハッシュ（rehash）を行います。
キーの削除操作（`remove`）では、墓石（tombstone）マークを残す方式をとらず、理想的な格納位置との距離を計算して後続のクラスタを前方へ手繰り寄せる後方シフト削除（backward-shift deletion）を用いて索引テーブルを整理し、実データのエントリー配列からは末尾要素を空き位置へ移動する swap-remove を実行します。このため、コレクションの要素列挙順序は挿入と削除の履歴順序のみによって決定論的に定まり、ハッシュ値、テーブルサイズ、アーキテクチャ、あるいは乱数シードの差異に依存しません。
デフォルトのハッシュ関数は、`Hash.hash`（64-bit FNV-1a）の出力を `fmix64` で攪拌（アバランシェ効果）させた値を用います。これは FNV-1a の下位ビットが入力の下位ビットのみに依存する傾向があり、2 の冪乗サイズのテーブル添字としてそのまま用いると衝突が集中しやすいためです。
`with_seed` や `with_capacity_and_seed` で生成されたマップは `keyed` フラグを有効化し、シード値から SplitMix64 によって生成された 128-bit のキー（`key0`、`key1`）を入力とする SipHash-1-3（`HashMap.sip13`。64-bit ワードを入力とする純粋関数）を用いて最終的なダイジェスト値を算出します。
`randomized` および `try_randomized` は、`Random.next_u64` からシード値を取得する `IO` アクションであるため OS API のカテゴリに属し、デフォルトの wasm32 では `E2000` で拒否され、ネイティブおよび `--wasm-host wasi` 環境でのみ使用可能です（安全なシード値を取得できない場合に固定シードへ勝手に縮退することはありません）。なお、シード付きマップは固定テーブルに対する意図的なハッシュ衝突攻撃を防ぐ効果を持ちますが、元の `Hash.hash` の段階で 64-bit ダイジェスト値そのものが衝突するキーに対しては無力であり、またシード値が推測された場合は防御効果が失われます。したがって、デフォルトのマップは暗号学的な HashDoS への完全な耐性を保証するものではありません。
キーの所有権を消費しない読み取り操作（`contains_key_ref`、`get_ref`、`at_ref`、`remove_ref` など）は `ref 'key` を受け取り、`at_ref {r s}` における region 契約によって、戻り値の参照はマップ自身の借用期間のみに拘束されます。キー型の `Eq` が反射律を満たさない場合（NaN など）は、`hash_of` 内部のアサートによって安全にトラップします。`longest_probe` は理想スロットから最も離れたエントリーの探査距離を返す診断用メソッドです。なお、SIMD による群探査（F08）、テーブル縮小、および集合演算（union／intersection）は現時点で未実装です。
検証は `cargo test --locked --test hash_map` および `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri hash_map` で行われています。シード付きコンテナが OS ランタイムを要求せず、`randomized` のみが OS ランタイムを要求することは `tests/os_api.rs`、ネイティブ・WASI・デフォルト wasm32 での挙動は `tests/os.mjs` の `randommaps` で検査されています。

**Arena（C10）:** `Arena.Arena`・`Arena.Handle`・`Arena.Slot` も `stdlib.rs` の不透明な標準 record として登録され、実装は `std/Arena.tz` の Tsuzuri ソースだけです。専用の `Type` バリアント、ランタイムのファイル、リンク条件は増えません。
`Arena<'a>` は値を詰めた `values: Vec<'a>`、各位置の slot 添字 `owners: Vec<i64>`、slot の表 `slots: Vec<Slot>`（位置と世代）、空き slot の列の先頭 `free` からなる DenseSlotMap 方式で、`Arena.Handle<'a>` は arena ID・添字・世代の 3 つの `i64` です。
使用中の slot の世代はハンドルと同じ 0 以上の値、空き slot は次の世代 `g` を `-1 - g`（-2 以下）、退役した slot は -1 で持ちます。ハンドルの世代は負にならないので、`position` の世代の比較だけで使用中の slot に限られます。関数値の複製で arena ID ごと複製された 2 つの arena の間でも、一方のハンドルが他方の空き slot（`position` は空き列の次）に一致して誤った値を返したり、swap-remove で空き列を壊したりしません。
`Handle` の `'a` はどのフィールドにも現れない phantom な型引数です。record 宣言の検査は、不透明な標準 record に限って公開フィールド型の検査と未使用型引数の `E1024` を免除します（判定は宣言ごとに一度だけ行う）。`src/recursive.rs` はフィールドを型引数で置換して辿るので、`record Node { edges: Vec<Arena.Handle<Node>> }` は再帰的な値レイアウトになりません。
arena ID は std 専用の組み込み関数 `Arena.__next_id`（`Builtin::ArenaNextId`、`Checker::builtin` が std の `Arena` 以外からの使用を `E1022` で拒否）が採番します。`emit_builtin` は単相の定義と大域カウンター `@tz.arena.next_id = internal global i64 0` を一つの文字列で出し、`atomicrmw add ... monotonic` で 1 増やして、結果が正でなければ `@llvm.trap` します。一意性だけが必要で、カウンターを通じて他のメモリを公開しないので `monotonic` で足ります。
既定の wasm32（atomics 機能なし）では LLVM の WebAssembly backend が atomic 命令を通常の load／add／store へ下げ、`--wasm-feature threads` では `i64.atomic.rmw.add` になります。どちらも WASM の import は増えません。Arena を使わないプログラムの IR は変わりません。
std の Arena 関数は他の std の generic 関数と同じく利用者コードから到達した要素型ごとに特殊化され、特殊化の上限（65,536 件）に数えます。検証は `cargo test --locked --test arena` と `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri arena`（`TSUZURI_TSAN=1` での並列採番を含む）で行っています。

**Rc／Arc（C10 Phase 2）:** 共有ポインタは `Type::Shared(Box<Type>, SharedKind)`（`Rc`・`RcWeak`・`Arc`・`ArcWeak`）で、std のソースを持たない組み込み型です。`resolve_type` が `Rc<T>`・`Rc.Weak<T>`・`Arc<T>`・`Arc.Weak<T>`（`std::` 付きも）を `Vec` と同じく宣言の解決より先に読み、`builtin_type_head` が公開型の検査・型エイリアスの展開・制約の収集で名前の解決を飛ばします。型名 `Rc`・`Arc` は `Vec` と同じく利用者の宣言に使えません（`E1001`）。関数は `Builtin::RcNew`〜`Builtin::ArcPtrEq` の 18 個で、`Builtin::shared_kind` と `SharedOperation` が `Rc`／`Arc` の同じ処理を引きます。`new` は予約語のまま、parser の `dot_member` がドットの後ろでだけメンバー名として読みます。
性質: `is_copy` は偽、`needs_drop` は真、`contains_reference`・`carries_loans`・所有権の `owned` は中の値に従います。`can_send` は `Arc` で値が `Send` かつ `shareable` のときだけ真で、`can_capture` は `Arc` で値が `shareable` のときだけ真です。`Type::shareable` は格納グラフ（`stored_all`）に `Rc`／`Rc.Weak`、`Type::Handle`、Copy でない dyn、`Owned.Function` がないことで、`Arc` を持つ複数のタスクが `Arc.get` の借用を通してホストのハンドルを同時に使うことを防ぎます（F10 が `Sync` に置き換えます）。関数値の型は捕捉を表さず、どの関数値も `Send` なので、`Rc` を関数値に入れないことでタスク間の非 atomic な計数を防ぎます。拒否のメッセージは `holds_rc` と `holds_unshareable_arc` で選びます。`Validation::check` は排他参照を含む中身を `E1005` で拒否し、`Layouts::size` は共有ポインタを 8 バイトとして中を辿りません。
再帰の解析（`src/recursive.rs`）は共有ポインタを格納のグラフに含めます（`visit`・SCC の辺・`stored_all`・`reaches`）。型引数を変える再帰（`E1017`）は、ジェネリックな宣言を自身の型パラメーターで解析するとき（`Graph::generic_root`）だけ、同じ宣言の別の具体化への到達として検査します。具体型を根とする解析では比較しないので、`Rc.upgrade` が返す `Maybe<Rc<Node>>` が `Node` の `Maybe<Rc.Weak<Node>>` に出会っても拒否しません。ほかの根の解析が先に到達した、自身のパラメーターのままの宣言の結果はキャッシュしないので、検査は宣言の順序によりません。増え続ける展開はノード 4096・深さ 128 の上限で止まり、経路に同じ宣言の別の具体化があれば `E1017`（引数の変化）として報告します。共有ポインタは union のノードと同じくヒープへの間接なので、`Graph::shared` に記録した共有ポインタを通る循環には union を求めません。値が有限かの判定では `Shared(T)` は `T` と同じです（`record Loop { next: Rc<Loop> }` は `E1010`）。循環の中の型は再帰型になり、性質の計算は既存の `stored_all` の経路を通ります。
生成（`src/llvm_shared.rs`）: 値は `ptr` で、`canonical_type` は `rc[T]`・`rc.weak[T]`・`arc[T]`・`arc.weak[T]`、debug 情報は基底型のないポインタです。`named_types` は共有ポインタの中の値も辿るので、`Vec<Rc<Maybe<string>>>` のように共有ブロックの中にしか現れない record／union の具体化にも型定義を出します。ブロックは `{ i64 strong, i64 weak, T }` で、`T` が再帰型を格納する（`Type::reaches_recursive`）ときだけ `%tz.rec.header` の 2 語を前に置いた `{ ptr, ptr, i64, i64, T }` です。
`drop_shared` はヌル（ムーブ済み）を飛ばし、強い数を減らして 0 なら値を drop してから `release_shared_block` で弱い数を減らし、0 ならブロックを `@tz.free` します。前置きのあるブロックは、値の drop の代わりに action `@"tz.shared.drop.{rc,arc}.<T>"` をブロックに書き、`drop_pending` があれば `@tz.rec.enqueue`、なければ `@tz.rec.drop` で再帰型と同じ待ちリストに積みます。action は `drop_pending` を付けて値を drop するので、`Rc` を通る長い鎖も再帰しません。action の型は `Globals::shared_types` に集め、`llvm_recursive::emit_helpers` が再帰型の helper と交互に、どちらも増えなくなるまで定義します。組み込み関数の本体は別の `Globals` で出力するので、`emit_typed_builtin` は本体が登録した再帰型と共有ブロックの型を共有の `Globals` へ移します。
`clone_value`（関数値の環境の複製）は強い数か弱い数を 1 増やして同じポインタを返します。増加は `i64` の最大値を超えるとトラップします（`TrapKind::NumericRuntime`）。`Arc` の増加は `atomicrmw add ... monotonic`、減少は `atomicrmw sub ... release` で、0 にしたタスクは値とブロックを壊す前に `fence acquire` を置きます。`Arc.try_unwrap` は `cmpxchg 1 → 0`（monotonic）の後に `fence acquire`、`Arc.upgrade` は強い数が 0 でない間 `cmpxchg n → n + 1`（成功は acquire）を繰り返し、計数の読み出しは `load atomic ... monotonic` です。`Rc` は atomic 命令を使いません。既定の wasm32 では LLVM が atomic 命令を通常の命令へ下げます。
Rc／Arc を使わないプログラムの IR は変わりません。検証は `cargo test --locked --test rc` と `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri rc`（`--wasm-feature threads` の WASM を `createThreadPool` で実行する検査を含む）、`TSUZURI_TSAN=1` での同じ suite です。
**正規表現と Unicode の表（D09）:** `Regex` は `std/Regex.tz` の Tsuzuri ソースだけで書いた Pike VM で、コンパイラに専用の型・構文はありません（`stdlib.rs` の予約名と不透明レコードへの登録、`Type::is_noncopy_record` による非 Copy 化だけ）。
`compile` は明示的なスタックで構文木（1 本の `Vec<i64>` に 6 語ずつの節点と子の列）を作り、節点ごとの命令数を飽和計算して上限を検査してから、`def rec emit` が 3 語 1 命令の `[i64]` を書きます。`[...]` は同じエスケープを 1 回だけ読み（2 回目からは表を読まず何も足さない）、閉じた後に併合・畳み込み・否定した区間を `add_class` が class の区間の合計の上限と比べます。照合は `search` と `add_thread` が呼び出しごとに確保する 1 本の `[i64]` の作業領域（手数カウンター、2 本のスレッドリストの dense／sparse 集合と捕捉表、作業用と最良の捕捉、`add_thread` の明示的スタック）を排他スライスで更新し、照合の内側では確保しません（`tests/features.mjs` の `regex` スイートが IR で検査）。
Unicode の表は `scripts/generate-unicode.mjs` が UCD 17.0.0 の入力（SHA-256 を固定）から生成する `src/runtime/unicode.ll` の `internal` 定数（`[N x i32]`、19 個）と、表番号で分岐する `@tz.unicode.length`／`@tz.unicode.entry`（範囲外はトラップ）です。
表は一般カテゴリー・正準結合クラス・書記素（`Grapheme_Cluster_Break`、`Extended_Pictographic`、`InCB`）・単語（`Word_Break`、`Extended_Pictographic`）の連続区間（`start << shift | value`）、二値 property の閉区間、分解（キー、`(符号位置または pool の位置) << 6 | 長さ << 1 | 互換`、pool）、一次合成の 3 つ組、大文字小文字の対応の連続した組（`start, count, stride, delta`）、`SpecialCasing.txt` と `CaseFolding.txt` の状態 F の完全な対応（`code << 2 | 種類` と pool）です。ハングル音節は表に載せず、`std/Unicode.tz` が算術で分解・合成し、書記素の LV と LVT を区別します。単語境界の WB6・WB7b・WB12 の先読み（Extend・Format・ZWJ の続きの次のスカラー）は後ろからの 1 回の走査で前もって求め、`tests/unicode.rs` は分割と大小変換の関数（`word_breaks`・`grapheme_breaks`・`converted`）の型付き IR にループの入れ子が無いことを検査します。std 専用の組み込み `Unicode.__table_length :: i64 -> i64` と `Unicode.__table_entry :: i64 -> i64 -> i64` がこれを呼び、std の `Unicode` と `Regex` 以外からの参照は `E1022` です。
`emit_target` は生成 IR に `@tz.unicode.` が現れるときだけ `unicode.ll` を連結するので、使わないプログラムの IR と WASM の import は変わりません。
組み込みの関数と `@tz.unicode.length`／`@tz.unicode.entry` は `alwaysinline` で、表番号が定数の呼び出し位置では `switch` が 1 つの表の読み出しに畳まれ、`-O3` では読まれない表を LLVM が削除します。
そのため std は表番号を実行時の値（捕捉した変数など）として渡す経路を作らず、表番号を引数に取る補助関数（`rank`・`run_map` など）は呼び出し位置ごとに定数で呼びます（大小変換の種類のように実行時の値で表を選ぶ箇所も、各分岐の中で定数の表番号を書きます）。trap の種類は `BoundsCheck` です。
`std/Unicode.tz` の正規化・分割・大文字小文字の変換は、入力をスカラーの `[i64]` に復号してから処理し、`string` と `utf8string` に符号化し直します（表の検索は二分探索）。
検証は `cargo test --locked --test regex`・`--test unicode` と `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri regex`・`unicode` です。`unicode` の期待値は、UCD の `GraphemeBreakTest.txt`・`WordBreakTest.txt` の全件と `NormalizationTest.txt` の抜粋（`tests/unicode-ucd.mjs`。`node tests/unicode-cases.mjs --write <UCD>` で再生成し、V8 が同じファイルの全行と一致することも確かめる）、全スカラーのカテゴリー・正規化・大文字小文字の変換（V8）、`CaseFolding.txt` の畳み込み、V8 の `Intl.Segmenter` と無作為な列です。後者のケースは `tests/regex-cases.mjs`（`--write` で `tests/fixtures/regex/Cases.tz` を再生成）が V8 の `u` フラグの正規表現で期待値を計算し、`\p{...}` の全スカラーの区間と `iu` の畳み込みの軌道を V8 と照合します。

**共有配列ビュー:** `ref [T]` は非所有の配列記述子 `%tz.array = { ptr, i64 }`（ポインタと要素数）として表現され、値のサイズは 16 バイトです。
一方、可変長ベクタ `Vec<T>` は `%tz.vec = { ptr, i64, i64 }`（データポインタ、要素数、確保容量）として表現され、構造体サイズは 32 バイトとなります。
`Vec` は常に非 Copy ですが、クロージャ環境の複製時などにおける内部的な clone は、確保容量を維持したまま独立したバッファとして安全に複製されます。デストラクタ（drop）は有効要素数（length）の範囲内の要素のみを解放します。
容量拡張に伴う再確保はネイティブでは `realloc`、WASM では隣接する空きブロックの結合・吸収を試み、不可能な場合に新規確保・コピー・旧領域解放を行います。
NULL ポインタや要素数 0 のケースはヘッダー読み出しよりも前に早期判定され、WASM のメモリ上限（デフォルト 16 MiB）は厳格に維持されます。
`ref mut [T]` およびその他の参照型は引き続き生のポインタ `ptr` として渡されます。共通のメモリレイアウト、クロージャ捕捉、関数引数、および戻り値もこの表現に統一されています。
配列スライス `Slice` は、元配列への借用を確立した後に範囲式の検査を行い、LLVM コード生成ではすべての境界検査が完了した後にのみ GEP（ポインタ計算）と長さの差分計算を実行します。
共有配列参照に対する非消費的な参照外しは配列ビューを読み出し、所有値としての参照外しは従来の配列ディープコピーを実行します。
要素への借用はビューから要素のアドレスを直接算出します。パターンマッチにおいて構造体全体の射影が必要な場合に限りエントリーブロックの一時記述子へ保存されますが、ユーザーコードに対してこの記述子への可変アクセスが公開されることはありません。スライスオブジェクト自体は clone や drop 時にバッファ操作を行いません。

**排他配列ビュー:** 排他スライス `ref mut [T..]` は型検査器の中では `Type::Reference(ArrayView(T), true)` であり、`ArrayView` は排他参照の参照先にだけ現れます。
表現の判定は `Type::slice_element` に集約されており、共有スライスと同じ `%tz.array` 記述子（16 バイト）を値として受け渡します。長さは固定なので、`ref mut [T]` のような記述子の置き場所へのポインタは不要です。単相化のシンボルでは `view[T]` と正規化されます。
排他スライスはすべて `TypedExprKind::Slice` として型付けされます。`ref mut values` や `ref mut [T]` から配列全体のビューへの変換（`src/check.rs` の `whole_view`）、ビューの貸し直しも同じ形になるため、所有権検査とコード生成の経路は一つです。
所有権検査は二相です。`src/ownership.rs` は範囲式を評価している間だけ元の place に共有の guard loan を置き（範囲の中の読み取りは許し、書き込みは拒否します）、評価後に排他 loan を作ります。`Array.split_at_mut` の二つの結果は実引数の貸し直しの loan を一つ共有し、範囲を区別しません。
スタックフレーム上の局所配列から排他スライスを作るときは、記述子を読む前に `own_heap_storage` が要素領域をヒープへ移します（`Array.write` が旧要素を解放するため、フレーム上の要素を `tz.free` に渡さないようにします）。参照外しを元とするスライスは移送しません。
`Array.write`、`Array.swap_in`、`Array.split_at_mut` は組み込みで、`checked_element_pointer` の境界検査（`TrapKind::BoundsCheck`）の後に旧要素を解放して格納します。`Array.sort_in_place` は `std/Array.tz` の関数で、挿入整列と SymMerge による作業領域なしの安定整列です。
`Parallel.for_each_chunk` は `src/llvm_parallel.rs` の `parallel_chunks` が `ceil(length / size)` 個のチャンクへ分け、各 worker へ互いに素な記述子を渡します。

**固定長配列:** `[T; N]` は型検査器の中では `Type::FixedArray(element, length)` で、長さは `Type::Length(n)` または長さパラメーター `Type::Variable("#N")` です。長さを型として持つので、型置換・単一化・単相化・別名展開は型変数と同じ経路を通ります（`Type` の大きさは 4 ワードのままです）。構文木では `TypeExprKind::FixedArray` と `TypeExprKind::Length` で、`const N: i64` は型パラメーター名 `#N` として登録されます。
長さの解決は `src/check.rs` の `resolve_length` に集約されています。名前の長さは、整数リテラルを初期化式に持つ `i64` 定数（`Names::length_constants`）、型宣言の長さパラメーター、関数の暗黙の長さパラメーターの順に解決し、1024（`MAX_FIXED_ARRAY_LENGTH`）を超える長さは `E1010` です。
LLVM では `[N x T]` の値として表現し、タプルと同じくフレーム上へ直接置きます。`llvm_frame::stack_size` と `Layouts::size` は要素サイズの `N` 倍で見積もり、単相化のシンボルでは `fixed[N,T]` と正規化されます。
添字は `fixed_element_pointer` が GEP で要素のアドレスを求め、`N` 未満の整数リテラルの添字では境界検査の分岐を出力しません。clone と drop は非 Copy 要素のときだけ要素ループ（`array_loop`）を生成し、名前付きの値から `ref [T]` への変換（`fixed_array_view`）は `%tz.array` 記述子を作るだけで要素を複製しません。
`FixedArray.init` の長さは `src/polymorph.rs` の `FamilyKind::FixedArrayElement` が期待型から決めます。直接の呼び出しは `fill_fixed_array` がフレーム上の値へ初期化関数を昇順に適用するインライン展開になり、関数値として使う場合は結果の配列型ごとの組み込みラッパーが同じ展開を行います。
公開 ABI では、スカラー型の固定長配列フィールドを C 構造体の配列メンバー `T name[N]`（LLVM では `[N x abi]`）へ正規化します。ISO C に長さ 0 の配列はないので、`[T; 0]` のフィールドを持つレコードは公開できません（E1008）。

**dyn 値:** `dyn C` は型検査器の中では葉の `Type::Dyn(Box<DynType>)` で、`DynType` はクラスの正規名（`Class::name`）の列と印 `copy`・`send`、region を持つかどうか（`borrowed`）を持ちます。型の性質（`is_copy`、`can_send`、`carries_loans` など）はこの印だけで決まります。
構文木の `TypeExprKind::Dyn` は parser が `Program::dyn_types` にも記録し、`Classes::collect` の最後に dyn 互換規則（`Classes::dyn_slots`）を出現順に検査して `E1028` を報告します。
`Classes::dyn_instances` は、書かれた互換な dyn 型ごとにディスパッチする各クラス `X` の `X<dyn ...>` インスタンスを合成し、本体を `ExprKind::DynDispatch { slot, slots }` にして通常のインスタンスの経路へ流します。このため制約付きジェネリック関数は `'a = dyn C` で一度だけ具体化されます。
`Dyn.of` は特殊化で `(vtable キー, 格納型)` ごとに slot 関数を解決して `CheckedModule::vtables` に記録します。vtable キーは `DynType::vtable_key`（`send` と `borrowed` を消したもの）で、組み込みの実装は組み込みラッパー関数を slot にします。アップキャストは `dyn_layouts` が元の vtable の slot 関数を使い回した受け先の vtable を不動点まで足し、`CheckedModule::dyn_layouts` に各キーの slot 数とアップキャスト先を持ちます。
LLVM では値を `%tz.dyn = type { ptr, ptr }`（data、vtable）とし、この型は dyn 型を書いたプログラム（`CheckedModule::uses_dyn`）だけがヘッダーへ出します。data は `@tz.alloc` で `storage_layout`（64-bit の上界）の大きさを確保します。
vtable は `internal unnamed_addr constant` の `{ ptr drop, ptr clone, i64 size, i64 align, [N x ptr] slots, [U x ptr] upcasts }`（clone は `Copy` 印のないキーで null、upcasts はアップキャスト先があるときだけ）で、記号は `@"tz.vtable.{クラス}[{格納型}]"` です。
slot は関数ごとの adapter `@"tz.dyn.slot.{関数}"` を指します。adapter は `(ptr data, 残りの引数) -> 結果` の型で、受け手を共有配列なら `%tz.array` の読み出し、参照なら `ptr` のまま、値なら読み出して領域を解放してから実装を直接呼びます。そのためディスパッチ側の間接呼び出しの型はメソッドだけで決まり、WASM の `call_indirect` の型検査と一致します。
drop slot `@"tz.dyn.drop[T]"` は `drop_value(T)` と同じ drop glue（利用者の `Drop` を含む）の後に領域を解放し、clone slot `@"tz.dyn.clone[T]"` は新しい領域へ `clone_value(T)` します。値の drop と clone は共有の `@tz.dyn.drop` / `@tz.dyn.clone` が vtable を読んで呼びます。vtable・adapter・drop 関数は全関数の後に `BTreeMap` の順で出力します。

**境界検査の省略:** `ranges.rs` は関数ごとに 1 回、型付き IR をワークリストアルゴリズムで走査して配列添字に関する不変事実 `RangeFacts` を構築します（走査ノード数が 65,536 個を超える巨大な関数については解析を打ち切り、すべての境界検査を安全に残します）。
`Type::Array` に対する添字アクセスにおいて、閉じた静的規則のみによって「添字が必ず配列の有効範囲内に収まる」と証明できた場合に限り、境界検査の条件分岐コードの生成を省略します。証明規則として対象となるのは、`0 .. len - 1` および `len - 1 .. -1 .. 0` のループ（`len` は `.length` または std の `Array.length`）、定数境界によるループ、リテラルや `new` による静的定数長の配列、ならびに `if i >= 0 && i < len` の then 分岐の内部です。
配列のローカル変数は、関数内で 1 回だけ宣言され、再代入も可変借用も行われない場合にのみ「安定（変更されない）」と判定されます。`Vec`、リスト、文字列、`ref mut [T]`、`Array.set` などの組み込み関数呼び出し、および公開 ABI のエントリーポイントは解析対象外であり、境界検査がそのまま残されます。
境界検査を省略した場合でも、要素アクセス用の `getelementptr inbounds` および `load` 命令はそのまま出力され、`llvm.assume`、`!range`、`nsw`、`nuw` などの未定義動作を招きやすい危険なフラグを新設することはありません。この解析は LLVM 最適化よりも前のコンパイラフロントエンドで実行されるため、`-O0`、`-O3`、ネイティブ、WASM のすべてのビルド構成で完全に同一の判断が行われ、省略された検査箇所はトラップ情報テーブルにも出力されません。残された検査箇所における `TrapKind::BoundsCheck` の種別やソース位置情報にも変化はありません。
境界検査を省略したメモリアクセスを含む整数リダクションループに対しては、`hint_loop` による `llvm.loop.unroll.enable`（ループ展開ヒント）の付与を意図的に抑制します。これは、検査が消えたループに対して過度な展開ヒントを付与すると、LLVM がベクトル化（Vectorize）よりも前にループを無理に展開してしまい、結果として実行速度が低下する現象（[実測データ](benchmarks.md#境界検査の残り方f12)）を防ぐための重要な設計です。境界検査が残る通常のループに対するヒント付与規則は変更していません。
検証は `cargo test --locked --lib ranges`、`cargo test --locked --test bounds_checks`、および `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri bounds_checks` によって厳密に行われています。

**比較の借用:** `Eq` および `Ord` のメソッド型は共有借用（`ref`）を受け取るシグネチャを持ち、比較演算子における所有権検査はすべての型において `Use::Read`（読み取り）として処理されます。
組み込みスカラー型以外の比較演算子は `Call(method, BorrowOperand(left), BorrowOperand(right))` へと単相化され、被演算子がメモリ上の場所であればポインタを、一時値であれば `frame_value` によるスタック値をエントリーブロックのスロットへ保存して渡します。
一時値は関数呼び出しの完了後に、左から右への順序で `drop_framed` により安全に解放されます。ソースコード上の `Borrow` 制約の意味論が変害されることはありません。
数値や文字列の組み込み演算子は直接の lowering 経路を維持し、メソッド値ラッパーのみが借用を参照外しして同一の組み込み命令を実行します。

**型別名:** `Names.type_aliases` は宣言元モジュール、可視性、および右辺の `TypeExpr` を保持し、レコード型や union と同一の名前解決規則を使用します。
`TypeAliasExpansion` は右辺の型式および型引数を各モジュールのコンテキストで正しく解決したうえで型式を展開します。型クラス制約の適用も型式として保持され、`inline_constraints` および格納型の制約検査へと引き継がれます。
循環参照は `E1024` エラー、ネスト深度 128 または走査要素数 4096 の超過は `E1017` として拒否され、展開結果に対して型サイズの有界性検査（`bounded_type`）が適用されます。
未使用の型別名宣言であっても厳密に型検査が行われ、公開（public）別名の右辺に非公開（private）型が出現していないかを検査する漏洩チェックも漏れなく実施されます。
型付き IR や LLVM コード生成のフェーズには型別名専用の表現は残存せず、インスタンス解決や関数の特殊化ではすべて展開後の具象型が使用されます。

**型パラメーターの構文:** レコード、union、型別名、型クラスの宣言、型適用、制約、インスタンス、および `Task<T>` の記述は、すべて共通の山括弧 `<...>` とカンマ区切りの構文で解析されます。型名と `<` は空白を挟まずに隣接させ、型引数の内部では完全な `type_expr` を解析するため、入れ子の型適用、関数型、借用型を記述する際に追加の丸括弧で囲む必要はありません。
空の型引数リスト `<>` は拒否されますが、末尾のカンマは許容されます。型パラメーターの個数およびネスト深度には既存の最大 128 の上限が適用されます。
型の閉じ括弧 `>` をパースする際は、トークンストリーム中の `>>`、`>>>`、`>=` から 1 文字ずつ `>` を消費することで、合成トークンを挿入することなく正確なソース位置（Span）を保持します。式の比較演算子やビットシフト演算子は従来のトークンとして解析されます。
旧来の空白区切りの型引数構文は受理されません。暗黙の全称量化、型推論、型付き IR、および LLVM 上の正規型名には影響しません。

**ジェネリックなレコード:** 構文 AST 上の名前付き型適用は `TypeExprKind::Apply(head, args)` に一元化されており、`Add<'a>` のような型クラス制約と `Pair<i64, string>` のような型の具体化は、型解決のフェーズで識別子の種類に基づいて正確に分類されます。
クラス名とレコード名は同一の名前空間を共有するため、未知の型名、型引数の個数不一致、または非ジェネリック型に対する型引数の適用は `E1004` エラーとなります。
型付き IR におけるレコード型は `Type::Record(id, args)` として表現され、非ジェネリックなレコードは空の引数列を保持します。
型引数列は `Box<[Type]>` として内部でヒープ確保して保持することで、`Type` 列挙型全体のサイズを 32 バイトに抑え、2 MiB スタックのテストスレッド環境でも深い式の再帰検査でスタックオーバーフローを起こさないように設計されています。`TypeExprKind::Apply` も同様の理由からヘッド名と引数列をボックス化して保持します。
レコード宣言のフィールド型には型パラメーターを含めることができ、`TypeContext` の `record_fields` および `record_field` が実引数で置換された具象フィールド型を返却します。Copy、drop、clone、loan、Capture、Send の各特性判定、所有権の部分 move、呼び出し特殊化の可変性判定、およびスタックフレーム配置の計算は、宣言時の抽象型ではなく必ず置換後の具象型に基づいて行われます。
メモリレイアウト計算（最大サイズ 64 KiB 上限および再帰型の検出 `E1010`）や、参照型の格納禁止チェックは具象型ごとに行われ、シグネチャ、型注釈、リテラル、および単相化後の式型の各境界で厳格に再検査されます。
LLVM コード生成では、非ジェネリックなレコードを従来どおり `%tz.record.M.N` として宣言順に出力し、続いて出力対象となる全関数のシグネチャ、ローカル変数、式型、および非ジェネリックレコードのフィールドから具象インスタンスを `BTreeSet<Type>` に収集し、入れ子のインスタンスまで推移閉包を求めたうえで 1 回ずつ決定的に定義します。
型名は `%"tz.record.M.Pair[i64,string]"` のような正規の型文字列（`array[T]`、`list[T]`、`tuple[T,U]`、`fn[T,U->R]`、`ref[T]`、`refmut[T]`、`task[T]` など）となり、ジェネリック宣言そのものの抽象型は LLVM IR に出力されません。
集合の順序と正規名はソースの記述順やハッシュ値に左右されず、ネイティブおよび WASM の双方で完全に決定論的な IR を生成します。

**レコード更新:** `{ base with field = value; ... }` の構文は、最初の式を通常のパーサーで読み取った後に続く `with` キーワードの存在によって識別されます。
`match` 式の内部に出現する `with` との混同を防ぐため、単純なトークン先読みによる判定は行いません。AST 上では `RecordUpdate` としてベース値と更新フィールドのリストを保持します。
型付き IR でも記述順序を保持する専用の `RecordUpdate` ノードへと lowering され、子要素の走査順序および所有権検査も記述順に厳密に従います。
LLVM コード生成では、元となるレコードとすべての更新値式を先行して評価し、置換対象となる古いフィールドの値を適切に解放（drop）した後に、LLVM の `insertvalue` 命令によってインプレースに値を差し替えます。
更新されない残りのフィールドに対しては通常のフィールド射影による余分なコピー（Copy）は行わず、元の集約値をそのまま引き継ぎます。

**共用体（union）:** 構文 AST の `UnionDecl` は、各 case ごとに 0 個または 1 個のペイロード型を保持し、`of` は case 宣言の内部でのみ有効な文脈キーワードです。union の型名は型クラスやレコードと同一の型名前空間に属し、case 名は同一モジュール内の型名、関数名、アクティブパターン名と衝突しない値の名前空間に属します。名前の衝突は宣言順序にかかわらず `E1001` エラーとなります。
型付き IR における型は `Type::Union(id, args)`、宣言情報は `CheckedModule.unions` の `CheckedUnion` で管理され、`TypeContext::union_payload(s)` が実引数で置換された具象ペイロード型を返します。Copy、drop、loan、Capture、Send などの特性は、レコードと同様に置換後の全ペイロードから構造的に判定されます。
関連する IR 式ノードとして、値の構築を行う `Construct { union_id, case_id, payload }`、case をファーストクラスの関数値として扱う `CaseConstructor`、32-bit 整数の識別タグを表す `UnionTag`、および case のペイロードを取り出す `UnionPayload` が用意されています。
全引数が完全に適用された case コンストラクタ呼び出しは、`Checker::call_kind` によって即座に `Construct` へと変換され、不要な関数呼び出しのオーバーヘッドを残しません。
完全適用されず関数値として残存した `CaseConstructor` は、再帰検査および単相化の完了後に `closures::lower` が `(union ID, case ID, 具象型引数列)` ごとに 1 つの非公開関数 `$case.M.U.C[.$mono.N]` へと置き換えるため、再帰呼び出しの診断メッセージに内部合成関数が漏洩することはありません。
case パターンマッチは、タグの等価判定 `Test(UnionTag(subject) == case)` とペイロード射影 `UnionPayload` へと分解されます。所有権検査において `UnionPayload` は、コレクションの要素（`ELEMENT`）と区別された独立したメモリ場所成分 `PAYLOAD` として追跡されます。
ペイロードの move は union の部分 move として扱われ、移動元のペイロード領域は即座にゼロクリアされるため、後続のスコープ終了時に union 全体を解放する際にも、移動済みのペイロードが二重解放される危険はありません。

メモリレイアウトの保守的な見積もり（`Layouts::union` および `llvm_frame::stack_size` で共有）は、ペイロードなしの場合は 8 バイト、ペイロードありの場合はタグ用の 16 バイトと最大ペイロードサイズを 16 バイトアライメントに切り上げた値の合計となり、64 KiB のサイズ上限および再帰型の検出（`E1010`）が具象型ごとに実施されます。再帰的な union は後述のヒープ所有ノード表現へと移行します。
LLVM 上でのメモリレイアウトは `UnionLayout` によって決定されます。すべての case にペイロードが存在しない場合は単なる `i32`（`Enum`）、すべての case のペイロードが同一の LLVM 型である場合は `{ i32, T }`（`Common`）、それ以外の不均一なケースでは `{ i32, [K x i128] }`（`General`）となります。
ここで $K$ は、64-bit ターゲットにおける正確な最大格納サイズ（i128 を 16 バイト境界で整列）を 16 で切り上げた整数値であり、ペイロードの読み書きは領域への GEP とペイロード型による `load`／`store` 命令によって行われます。
LLVM 型名は `%"tz.union.M.Shape"` や `%"tz.union.M.Maybe[i64]"` のようになり、ジェネリックレコードと同様に `named_instances` の推移閉包を求めて具象インスタンスのみが決定論的な順序で 1 回ずつ定義されます。
メモリの解放および複製処理では、タグ値に対する `switch` 命令を用いて現在活性化している case のペイロードのみを処理し、非活性のペイロード領域を誤って読み出したり解放したりすることはありません。
なお、`Construct` による union の生成はスタックフレーム管理の対象外であり、union の値は `new` を付けないリテラルであってもフレーム領域を持たず直接構築されます。

**再帰構造の解析:** `recursive::analyze` は、具体的な record および union と、タプル、配列、リスト、Vec などの格納エッジを包括的に走査します。
関数値、タスク、および借用参照は格納グラフの境界として扱われます。同一の具象型へ戻る循環参照と強連結成分（SCC）を分類し、有限値の不動点、union を経由しない不健全な無限サイズ循環、および型引数が無限に膨張・変更する再帰定義を厳密に検出・拒否します。
解析結果は宣言ごとの型引数キャッシュに保存され、元の宣言、`Type`、または正規の名前文字列を書き換えることはありません。
レコード型は常にインライン展開のまま維持され、再帰 SCC に含まれる union のみがポインタ `ptr` 表現へと下げられます（最初の nullary case は NULL ポインタとして表現されます）。
再帰ノードは `{ next, drop_action, clone_action, tag, payload }` の構造を持ち、タグとペイロードは独立した GEP 命令でアクセスされます。
コンストラクタの直接適用と関数値経由の構築は同一の `construct_value` を使用し、ペイロードの評価完了後にヒープメモリを 1 回だけ確保します。
部分 move が行われた場合は子要素の所有スロットがゼロクリアされ、親ノードは残存するペイロードとともに 1 回だけ安全に解放されます。

`runtime/recursive.ll` は再帰データ構造を処理する共通の反復ループを提供し、`llvm_recursive` は具象型ごとの解放・複製ステップ関数を生成します。
デストラクタ（drop）では、解放予定のノード自身の `next` ポインタを作業待ちキューとして再利用し、子ノードをキューに登録してから親ノードを `free` するスタックレスアルゴリズムを採用しているため、深い再帰構造の破棄時にもコールスタックを一切消費せず、追加の動的メモリ確保も行いません。
クローン（clone）処理でも複製先ノードを作業待ちキューとして利用し、一時的にステップ情報とコピー元アドレスをヘッダーへ保存し、ペイロードのコピー完了後に通常の drop／clone ヘッダーへと復元します。外部の作業用リストをヒープ確保することはありません。
動的コレクションはその場で走査され、再帰する子要素は同一の待ち行列へ登録されます。
これらの LLVM 専用ヘルパー関数群は到達した関数から要求された分だけ生成され、公開 ABI、ユーザー定義関数、警告、あるいはテストルートへ混入することはありません。
ネイティブ環境における 100 万ノードのスタックレス解放、ならびに WASM の上限メモリ内での深い構造の複製とデフォルト 16 MiB 超過時の安全なトラップが検証されています。

**利用者定義の解放（B07）:** 組み込み型クラス `Drop` のインスタンスヘッドとして宣言された型には、`CheckedRecord::user_drop` または `CheckedUnion::user_drop` のフラグがセットされます。
このフラグはインスタンス収集の直後、スーパークラスの検証および関数本体の型検査よりも前の段階で付与され、`Type::has_user_drop` を通じて `is_noncopy_record`（Copy 判定で最初に参照される）、`needs_drop`、および `can_capture` へと確実に伝播します。
単相化の完了後、特殊化された関数の型シグネチャが所有するすべての Drop 型（フィールド、ペイロード、コレクション要素、Task の結果。参照型や関数型は辿らない）を収集して `drop` メソッドを特殊化し、不動点解析の結果を `CheckedModule::user_drops`（`BTreeMap<Type, usize>`）に格納します。`Drop` インスタンスが存在しないプログラムではこの走査は行われず、生成される IR も変化しません。
所有権検査は `Place::through_drop` により、Drop 型の境界を通過するフィールドやペイロードのパスを記録し、その内部からの部分的な move、Drop 型に対する `RecordUpdate`、および `drop` メソッド本体における引数全体への再代入を厳格に禁止します。
呼び出し先での値の置換を防ぐため、`drop` メソッドの本体内では引数値全体の排他的な再借用や、`ref mut` 引数そのものの move も拒否されます（`Checker::drop_root`）。
LLVM は move 済みのメモリ領域を `zeroinitializer` でクリアし、その解放処理を安全な no-op（無処理）とするセマンティクスを徹底しています。そのため、非再帰の Drop 型はレコードのフィールド末尾（union ではタグとペイロードの直後）に `i8` の生存フラグ（liveness flag）を保持し、値の構築時（レコードリテラル、スタックフレームの集約、`construct_value`）に `1` に設定します。すべての case が引数なしの Drop union であっても、単純なタグ整数ではなく `General(0)` のレイアウトが選択されます。
デストラクタコード（`drop_value` および `drop_framed`）は、生存フラグが 0 でない場合にのみ値をエントリーブロックのスロットに配置してユーザー定義の `drop` メソッドを直接呼び出し、読み直したフィールド値を宣言順に解放します。
ゼロクリアされた領域では生存フラグが 0 となるため `drop` は呼び出されず、各フィールドの解放も従来どおり安全にスキップされます。一時的な Copy 型のフィールドやペイロードを読み出す際は、値を破壊することなく全体を安全に解放します。
再帰的な Drop union では先頭の引数なし case もノードとしてヒープ上に配置され（NULL ポインタは move 済みのみを表現します）、型ごとの drop ヘルパーが各ノードに対して `drop` を呼び出してから子ノードを待ち行列へ追加します。
`user_drops` に登録された関数群は本体コードから明示的には呼び出されないため、デッドコード削除で誤って除去されないよう到達可能性解析のルート集合に追加されます。なお、Drop を実装した record 型は C 言語 ABI のスカラーレコードとしては扱われません。

`use` 束縛は、`syntax::Binding::using` フラグを持つ特殊な `let` 束縛であり、型検査によって対象の値の型が `Drop` を実装していることを必須とします。コンピュテーション式内の `use!` は、生成された継続関数の引数を `use` で再束縛し、`task` ビルダーにおける `use!` は `let!` と同様の `TaskRun` として処理されます。
`Owned.drop`、`Owned.function`、および `Owned.call` はコンパイラ組み込み関数として提供され、`std/Owned.tz` では不透明な非 Copy 型である `Owned.Function<'a, 'b> { run: 'a -> 'b }` のみを宣言します（std 関数を無用に増やすと生成関数の採番がズレて既存の IR 決定性が崩れるのを防ぐためです）。
`Owned.function` の引数に直接記述されたラムダ式は `TypedExprKind::Lambda::owned` となり、自由変数の捕捉に対して通常の `Capture` 制約ではなく `Send` 制約を課します（単相化後の再検査でも同様です）。
`closures::lower` は持ち上げられた関数に対して `owned_captures` フラグを設定し、`TypedExpr::consuming_use` を通じて、非 Copy な捕捉値（デストラクタを持たない `extern type` のハンドルを含む）が関数本体によって不用意に move されないことを静的に検証します。
LLVM はその関数の捕捉引数を借用ローカル変数として生成し、apply アダプタは本体を呼び出した後、所有権を消費する呼び出しである場合にのみ `@tz.env.drop` を呼び出して環境を破棄します。
環境のクローン関数は生成されず（記述子の clone ポインタは常に NULL）、`can_borrow` フラグは偽に設定されて、既知クロージャの直接呼び出しや借用ワーカーの特殊化最適化からは除外されます。
`Owned.call` は環境の借用フラグ（`i1 true`）を指定して apply を呼び出します。また、`Owned.Function` 自体は `can_capture` を満たさないため、誤って `clone_value` へ渡されることはありません。

`match` 式のコンパイルでは、`llvm_control::SwitchPlan` が最適化を担います。ガード条件を持たないすべての match 節の判定条件が単一のタグ値（整数、bool、unit の場合は値そのもの）の定数比較であり、かつ各節の束縛が同一の射影パスを持つ場合に限り、対象領域からタグを 1 回だけロードする高効率な `switch i32` 命令へと変換します。
各節の束縛値は `UnionPayload` または構造体フィールドの射影ポインタから直接読み出され、データ構造全体の不要な別名（alias）を作成しません。
OR の各側の束縛経路は一致させ、条件を満たさない match は従来の順序付き検査に戻します。

**カリー化:** 関数シグネチャの型宣言には `def`、実装には `fn` または宣言に対応する `let` とラムダ式を使用します。
型の矢印記号 `->` は右結合、関数適用は左結合です。内部の `Type::Function` が保持する引数列は連続する矢印列を正規化した表現であり、非カリー化関数を意味するものではありません。実装側の `Signature.parameters` は各ワーカー関数が実際に受け取る引数列を保持し、途中で関数を返却する関数の評価を後続引数の評価より後へ遅延させることはありません。
すべての引数が完全に適用された既知のワーカー関数は直接呼び出されます。一方、部分適用された呼び出しや未知の関数値に対する適用では、`{ code, environment, clone, drop }` の 4 要素からなるクロージャ記述子と、1 引数ずつ処理するアダプタ関数が使用されます。
通常のアダプタ関数の内部引数順序は「値引数、環境ポインタ、`i1 borrow`」となっており、戻り値レジスタを次回の呼び出しの値引数レジスタとしてそのまま再利用できるように配慮し、ループ反復ごとの不要なレジスタ退避・移動を排除しています（なお、引数を持たない thunk や 1 回実行 Task の呼び出し規約は変更しません）。
借用が残存しないことを静的に証明できる途中段階では一時的な借用（loan）を終了させ、判定が困難な場合は安全側に倒して保守的に保持します。

**捕捉（キャプチャ）:** ラムダ式内の自由変数は一意な識別子（ID）によって検出され、ラムダ式自身は捕捉引数を受け取るワーカー関数へと持ち上げ（lambda lifting）られます。
変数捕捉時には Copy 値の複製または所有値の move が行われ、関数値をコピーする際には完全に独立した環境スナップショットが複製されます。
文字列やネストした内部関数値も環境ごとに独立してディープコピーされるため、ガベージコレクタ（GC）、参照カウント、および共有可変状態は一切不要です。
通常の Copy 値であっても内部に関数値を含む場合はデストラクタ呼び出しが必要となるため、コンパイラ内部において `is_copy` と `needs_drop` は同義ではありません。集約値のコピーや一時配列の添字アクセスにおいても、クロージャ環境の複製と解放の漏れが発生しないよう厳格に追跡されます。
Task 以外のクロージャにおいて捕捉変数が 1 個のみであり、かつ整数、bool、f32、f64 のビット幅がターゲット環境のポインタ幅以下に収まる場合は、環境ポインタ欄をヒープアドレスとしてではなく値のビット列そのものを格納する領域として活用します。`immediate_capture` 最適化がコード生成、読み出し、およびアダプタ関数のすべての箇所で同一の条件を判定し、`inttoptr`／`ptrtoint` や浮動小数点の bitcast のみを用いてインプレースに受け渡します（このフィールドをポインタとして参照外ししたり `free` したりすることは厳禁です）。
この即値環境の複製および解放には共有の恒等関数／no-op 関数が用いられ、ゼロ値によって環境フィールドが NULL となった場合でも正常な値として正しく識別されます。
所有値、参照型、複数変数の捕捉、およびポインタ幅を超える広幅値は従来のヒープ環境を使用し、wasm32 における `i64`／`f64` も従来経路で処理されます。
1 個の即値格納状態から部分適用によって複数変数の捕捉へと移行する際は、値を安全に復元して通常のヒープ環境へと移送します。
記述子の 4 フィールド構成、公開 ABI、および所有権／借用の基本規則は一切変更されません。
通常の apply アダプタは、内部の末尾引数 `i1 borrow` によって所有モードと一時借用モードを区別します。
所有モードは受け取った環境を消費し、後続の段階へ所有値を移動するか、ワーカー関数へ引き渡した後に環境メモリを解放します。
借用モードによる完全適用では既存の読み取り専用解析を利用し、解放が不要な捕捉値は通常ワーカーへ、所有権を持つ捕捉値は借用ワーカーへと引き渡します。
消費的な捕捉、部分適用、または特殊化予算の枯渇時には、アダプタ内部で独立した環境を複製して所有モードへと遷移します。
なお、Task の 1 回消費アダプタにはこの引数を追加せず、Task 専用の ABI を維持します。
共有参照の借用関係は、関数値や集約値のコピーを経由しても正しく引き継がれます。
排他参照（`ref mut`）を内部に保持する再利用可能なクロージャ環境の構築は `Capture` 制約によって拒否され、完全適用の呼び出し中にのみ一時的な保持が許可されます。
公開 ABI には環境ポインタを露出させず、必要に応じてすべての引数をフラットに受け取るブリッジ関数が生成されます。

**継続の特殊化:** 型検査および所有権検査が完了した通常の関数呼び出しを対象とし、コンピュテーション式の操作関数名などを特別扱いすることはありません。
関数引数が完全適用の呼び出し先（callee）としてのみ使われているか、あるいは他のエスケープしない引数やコレクション初期化にのみ使われているかを、相互再帰を含むコールグラフ上の不動点解析によって確認します。結果として借用を外部へ持ち出す関数は保守的に除外されます。
呼び出し先と捕捉変数数が静的に確定している関数値に対しては、`(関数 ID, 既知の関数引数列)` ごとに特化したワーカー関数を自動生成します。
一時的なクロージャ環境はエントリーブロックの `alloca`（スタック領域）に配置され、関数呼び出しの終了直後に捕捉していた所有値が安全に解放されます。
callee の解析において引数式を本来の評価順序より先に先行評価することは禁止されています（パイプライン演算子 `|>` のみは言語規則に従って左辺の引数を先に評価します）。

既知の callee が捕捉した所有値を読み取るだけであれば、そのプレフィックス引数の解放（drop）を行わない内部ワーカー関数を生成して呼び出します。
元のクロージャ環境および所有権は呼び出し側がそのまま保持し続けます。部分適用によって所有値を差し替える関数や、非 Copy な捕捉値を move する関数はこの最適化から除外されます。Copy な捕捉値を値として取り出す際に必要な複製処理は確実に維持され、借用ワーカーのプレフィックス引数を 1 回使用の所有値と誤認して勝手に move や drop を行ってはなりません。
通常の所有引数やローカル変数については、従来どおりスコープ終了時の drop が行われます。
後続の引数に可変参照や再代入が含まれている場合は、callee の完全なスナップショットをヒープに保持する通常経路が選択されます。
関数値そのものがスコープ外へエスケープしたり外部へコピーされたりする場合も、従来のスナップショット生成が維持されます。
呼び出し先が動的に決定される場合であっても、呼び出し対象が静的な場所（place）であり無害な引数であるならば、呼び出し中のみ環境を借用し、アダプタが安全な実行経路を選択します。
配列やリストの未知の初期化関数についても、保持された環境を一時的に借用し、コレクションの初期化完了後に元の環境を 1 回だけ解放します。

関数本体が引数をそのまま素通しして返却するだけの関数は透過的に扱われますが、単に識別子名が `Delay` や `Return` であるという理由だけで恒等関数とみなすことはありません。
元の AST 全体を通じて参照箇所が 1 箇所のみである所有ローカル変数は、消費的な読み出しの際に Copy 型であってもメモリ領域ごとインプレースに移動させることができます。
その際は移動元のスロットを即座にゼロクリアし、スコープ終了時の二重解放を防止します。複数回使用される変数、構造体の部分フィールド、および借用ワーカーへの入力に対してはこの最適化を適用しません。配列やリストの初期化関数が既知の callee である場合は、元の順序で初期化式の評価を 1 回だけ行い、各インデックスに対する要素生成関数をインラインで直接呼び出します。

追加生成されるワーカー関数は、順序付きキーとキューを用いて重複が排除され、プロジェクト全体で最大 1024 個までに制限されます。上限を超過した呼び出しは安全な通常経路へと戻されます。
既にスタック環境を借用しているワーカーの内部でこの上限に達した場合でも、通常のアダプタへ渡されるのは独立してヒープ上に複製された環境であり、スタック上の環境を誤って `free` してはなりません。関数記述子の 4 ポインタ表現および公開 ABI は常に厳格に維持されます。
通常の型エラーや所有権の move エラーを最適化によって暗黙のうちに揉み消して受理することは決してなく、数値範囲やメモリ確保サイズの安全性検査も完全に維持されます。

**数値:** 各整数ビット幅、符号の有無、および各浮動小数点形式は、それぞれ厳密に独立した型として扱われます。`byte`／`ubyte` は `i8u`、`sbyte` は `i8` の別名です。
リテラルの型は文脈から推論されるか明示的な接尾辞によって決定され、変数間の暗黙の型変換は一切行われません。
接尾辞のない整数リテラルは `GenericInteger` の推論変数として導入され、関数末尾まで具体的な型が定まらない場合はデフォルトで `i32` に決定されます（浮動小数点リテラルは `f64` です）。
浮動小数点数、decimal、または `bigint` が期待されるコンテキストに記述された整数リテラルは、最初からその型の数値リテラルとして解釈されます。
binary 浮動小数点リテラルをいったん f64 で丸めてから f128 へと拡大するような二重丸めは厳格に禁止されています。
decimal 型は BID（Binary Integer Decimal）形式に準拠し、有限値、非正規化数、符号付きゼロ、無限大、および NaN を正確に保持します。
ソフトウェア演算では、基数 2 または 10 の整数係数と指数を正確にデコードし、多倍長整数を用いて計算したうえで、目的の精度へと 1 回だけ最近接偶数丸めを行います。binary64 による安易な近似計算で代用することはありません。

`Display` および `Parse` は `Operation::Builtin` を持つ組み込み型クラスであり、通常の単相化されたメソッド呼び出しを `BuiltinInstance` の Display／Parse 呼び出しへと lowering します。標準の `Maybe` が存在しないカスタム std 入力環境では、Parse シグネチャの解決失敗を使用箇所でのみ報告し、無関係なモジュールの解析を不当に阻害しません。
`to_string` における独自の Display 呼び出しは、引数を借用して文字列表現を生成した後に通常どおり drop する型付き関数を生成します。
再帰検査の参照グラフにもこの Display の依存エッジが含まれます。`string` に対する消費的な文字列化処理は所有権をそのまま返却します。
`utf8string` の Display は UTF-16 文字列へと変換を行い、消費的な文字列化では変換完了後に元の UTF-8 メモリ領域を安全に解放します。

数値の文字列表現の入口は `tz_soft_format` に統一されており、コンソール出力、`Display`、および `to_string` が完全に同一の出力形式を共有します。
整数の文字列化では、100 で除算して 2 桁ずつ数字を生成し、値が 64-bit に収まる範囲内でのみ `u128` の除算を行い、以降は高速な `u64` 除算へと切り替えます。
各桁ペアは 32-bit 算術によって分解され、外部の大きな係数テーブルやホスト C ランタイムの文字列化関数には依存しません。
狭幅整数の解析における cutoff 計算も `u64` で行われ、整数の有効範囲、符号、および桁区切り文字の規則が厳格に維持されます。
数値の load／store には、対応する 8/16/32/64/128-bit 幅の固定サイズメモリコピーを使用し、アライメントやポインタのエイリアス関係を勝手に仮定しません。
従来の little-endian バイト処理は C 言語のフォールバックとして保持されており、生成される LLVM IR のターゲットは従来どおり little-endian を前提としています。
多倍長の作業領域を要する浮動小数点の表示・解析処理はインライン展開されない独立した内部関数に分離され、整数の処理経路におけるスタック消費量を極小に抑えています。
binary 浮動小数点の正負ゼロの表示、ならびに `inf` や `nan` の解析は、多倍長演算に入る前の段階で早期に完了します。
数値表示における ASCII 文字列出力は、検証済みの内部専用変換ルーチンによって直接 UTF-16 へと拡張されます。
一般的な UTF 変換では、事前の厳密な文字計数によって「全体が ASCII のみで構成されている」と証明された場合にのみ書き込みパスを単純化します。
非 ASCII 文字を含む変換であっても、入力全体を検証した後の書き込みフェーズでは内部の `validated` フラグを使用し、重複した境界検査を省略します。
入力データは同一の不変借用バッファとして扱われ、文字計数、単独の整形式（well-formed）検査、および文字修復処理では必ず未検証モードを使用します（外部入力を検証なしで直接受け入れるような安全性の甘い API は存在しません）。
UTF-16 と UTF-8 の一致比較にはバッファ境界内の 64-bit 一括比較を用い、辞書順比較は最初の不一致位置における符号なし 16-bit 単位の値で判定します。
binary 浮動小数点は、既存の `tzrt_big` による Dragon4 アルゴリズムの正確な境界区間計算から最短桁の文字列表現を生成します。
2 の冪乗で区間が狭まる下側境界、最小正規化数の例外、および偶数仮数の閉区間条件を明示的に処理し、同一桁数の候補が存在する場合は目標値との近接性および十進仮数の偶奇性に基づいて正しく選択します。decimal 型は既存の BID デコード処理を共有して末尾の不要なゼロを正規化します。
第三者製ライブラリ、外部係数テーブル、あるいはホスト環境の libc 変換関数に依存することはありません。

数値解析（Parse）の入口は UTF-16 の `string` を受け取り、4096 コード単位以下の ASCII 部分のみを一時バッファに切り出して処理します。
非 ASCII 文字や孤立サロゲートが含まれている場合は直ちに `None` を返し、不正な UTF-8 変換や暗黙の文字置換を行うことはありません。
`tz_soft_parse` は 4096 バイト以下の厳密な ASCII 構文を検証し、整数解析ではターゲット幅の cutoff 値と最終桁の値からオーバーフローを判定します。
十進整数の先頭に 8 桁以上の数字が連続する場合、境界内の 8 バイトを一括して検証・集約します。
基数 100,000,000 に対する cutoff と余りを計算して範囲内であることを確認したうえで値を累積し、区切り文字、非数字、または文字列末尾に達した時点で従来の 1 桁ずつのループへと復帰します。
8 桁の一括検査が失敗した場合は入力ポインタを進めず、符号、先頭ゼロ、桁区切り、非十進表記に関する文法規則を完全に維持します（特別な SIMD 命令や追加の CPU 要件は不要です）。
浮動小数点数の解析では、十進係数と指数を正確な有理数として既存の `pack` 関数へ渡し、目的のバイナリ形式へと 1 回だけ丸め込みます。
係数 4096 桁と形式ごとの指数範囲が `LIMBS` 配列内に安全に収まるよう、極端な指数値は拡大処理の前に overflow または underflow として早期分類されます。
解析失敗時は 0（`Maybe.None`）を返し、成功時のみ出力スロットを初期化します。LLVM は成功分岐でのみスロットの値を読み出し、標準 `Maybe` の Some／None タグと共通のペイロード領域を構築します。
表示用の一時バッファはエントリーブロックの `alloca` による 128 バイトのスタック領域であり、生成された ASCII 結果は `tz.string.from_ascii` を通じて所有権を持つ UTF-16 文字列へと変換されます。

**文字型:** `Type::Char` はすべてのビットパターンが有効値となる LLVM の `i16`、`Type::Utf8Char` は検証済みの Unicode スカラー値を表す `i32` として定義されます。
ソースコード上で異なるリテラル記法で記述された値も、型検査後は既存の整数定数ノードとして保持され、型情報によって区別されます。
値の比較は符号なし整数比較として行われ、`match` 式はそれぞれ `switch i16` または `switch i32` へと lowering されます。安易な整数キャストや C 言語 ABI への直接露出は禁止されています。
`runtime/character.ll` は Display／Parse および最大 4 バイトの UTF-8 出力ロジックを共有し、char 型におけるサロゲート文字の直接出力のみを適切に拒否します。
必要な場合にのみ既存のコンソール／文字列ランタイムとリンクされ、WASM のホストインポートを増やすことはありません。

**文字列:** `Type::String`（UTF-16）と `Type::Utf8String`（UTF-8）は、互いに独立した非 Copy（所有型）の文字列型です。
構文および型付き IR の `StringLiteral::Utf16(Vec<u16>)` は Rust の標準 `String` を経由せずに生データを保持するため、孤立サロゲートを欠落させることなく完全に表現でき、`StringLiteral::Utf8(String)` は検証済みの妥当な UTF-8 バイト列を保持します。`match` の定数パターン比較においても符号単位を失わずに正確に比較されます。
LLVM 上では、`%tz.string` および `%tz.utf8string` という独立した `{ ptr, i64 }` 構造体記述子（ポインタと長さ）が用いられ、長さフィールドはそれぞれ UTF-16 コード単位数、および UTF-8 バイト数を表します。
`std/String.tz` および `std/Utf8String.tz` の `length` 関数は、共有借用の `.length` を呼び出す通常の標準ライブラリ関数として実装されています。
記述子から長さを直接読み出すため、文字列長を取得するだけの操作でランタイム走査、メモリ複製、あるいは文字コード変換が発生することはありません。
UTF-16 の文字列定数は `[N x i16]` の配列としてバイナリに出力され、ホストマシンのエンディアンに依存した生のバイト列がそのまま埋め込まれることはありません。
UTF-16 バッファの確保時には、文字長が $2^{53} - 1$ 以下であることを厳格に検証したうえでバイト長（2 倍）を算出し、文字列連結時にも確実な上限チェックが適用されます。
インデックスアクセス、メモリ複製、および文字列連結は、各型専用の直接的な LLVM 命令として出力され、実行時に文字エンコーディングを動的に判別するような汎用ディスパッチのオーバーヘッドはありません。
`string` の辞書順比較は符号なし 16-bit 整数の順序に従い、暗黙的な Unicode 正規化は行われません。
`utf8string` の辞書順比較は符号なし 8-bit 整数のバイト順に従います。各モジュールの `compare` 関数と比較演算子は同一の型別ヘルパー関数を使用します。
文字列の検索、構築、およびスライス切り出しは、通常の std ソースコードと `Vec` の単相化機構を用いて実装され、必要なバッファ容量を事前に計算して 1 回で確保します。
`llvm_bulk.rs` の型付き組み込み関数が、string と `[i16u]` 配列、ならびに utf8string と `[ubyte]` 配列の間で所有記述子をゼロコピーで高速に移送します。
UTF-8 のバリデーション処理は、デコード失敗を値として返す内部ルーチンを共有しており、`from_bytes` は失敗時にメモリを解放して `None` を返し、厳密な復号 API はトラップを発生させます。
プロジェクト内で使用されていない std 関数は型検査のみを受け、ユーザーコードから実際に呼び出された関数のみが特殊化されるため、未使用の標準ライブラリコードが特殊化予算を無駄に消費することはありません。
UTF-8 へのエンコーディング変換およびコンソール出力時、孤立サロゲートが検出された場合は安全にトラップし、文字の置換処理は `String.to_well_formed` を明示的に呼び出した場合にのみ行われます。
所有権管理、スタックフレームからのヒープ移送、クロージャ環境への捕捉複製、およびデストラクタ（drop）のセマンティクスは、両文字列型で完全に統一されています。

**タスク:** `Type::Task(T)` は、非 Copy かつコールド（遅延評価）な 1 回実行の並行計算を表現します。
型検査において `task` 式は引数なしのラムダ式として扱われますが、自由変数の捕捉および計算結果に対しては通常の `Capture` 制約ではなく、スレッド間転送を保証する `Send` 制約が課されます。
参照を含むデータ構造や、借用を保持したクロージャ環境をタスク内へ捕捉することは禁止されており、タスク内部で生成された借用参照がタスク外へ漏洩することも防ぎます。
タスク生成時のクロージャ環境は所有権チェッカーによって厳密に検証されるため、持ち上げられた `is_task` ワーカー関数の捕捉引数は完全に閉じられた所有値として安全に取り扱うことができます（通常の関数値引数に対してこのような仮定を置くことはできません）。
`Task<T>` は外部への借用を一切運ばないという不変条件が、抽象本体の段階と具象化後の双方で徹底して維持されます。
通常の再利用可能なクロージャ環境へタスクを誤って捕捉させるようなコードは静的に拒否されます。

LLVM コード生成ではクロージャ記述子 `%tz.closure` の apply、environment、drop の各フィールドを再利用しますが、clone ポインタは常に NULL となり、タスク用環境のクローン関数が生成されることはありません。タスクを含む一時的な完全適用環境もクローン不可となります。
コンピュテーション式における `let!`／`return!` と `Task.run` は、同一の 1 回消費 lowering パイプラインを共有します。
`Task.parallel` 自体は配列の所有権を受け取るコールドタスクを生成し、実行時にのみ結果格納用の連続メモリ領域を確保します。
入力配列の各スロットを 1 回ずつ消費するコールバック関数は、戻り値の LLVM 型ごとに決定論的に重複排除されて生成されます。
コールバック関数の C 言語境界は `(void *context, uint64_t index)` のみに限定されており、集約値の内部 ABI を C 側に露出させることはありません。
すべてのコールバック処理が完了したことを確認した後に元入力配列のメモリ領域が解放され、初期化済みの結果配列が返却されます。
タスクグループのコンテキスト構造体を含め、反復処理で使用されるスタックメモリ（alloca）は関数のエントリーブロックに集約して確保されます。

ネイティブランタイムは `pthread_once` を利用して、`min(sysconf(_SC_NPROCESSORS_ONLN), 32) - 1` 本の常駐ワーカースレッドを遅延起動します。
CPU コア数の取得に失敗した場合は追加ワーカーを起動しません。タスクの要素数が 0 件または 1 件の場合はスレッドプールを起動せず、呼び出し元スレッドがその場でインライン処理します。
ミューテックスは FIFO 順のアクティブグループリストおよび次回処理インデックスの割り当てを保護するためにのみ用いられ、ユーザーコールバックの実行中にはロックを一切保持しません。呼び出し元スレッド自身も自グループのタスク実行を積極的に推進します。
残存タスク数（`remaining`）は、各コールバックの結果がメモリへ格納された後にアトミックな acq-rel RMW 命令で減算され、呼び出し元スレッドが acquire で 0 を確認した後にミューテックスの保護下でグループをリストから外します。
最後の減算を完了したワーカーはグループ構造体に二度とアクセスせず、共有の完了条件変数にシグナルを通知するのみであるため、スタック上に確保されたグループの生存期間を超えて不正メモリアクセスが発生することはありません。
`Parallel` の完全適用は専用の `TypedExprKind::Parallel` として扱われ、通常の関数呼び出しへ消去される前にコールバックおよび結果の所有環境が静的に検査されます。
カリー化されたコールバックにおける 1 引数／2 引数の適用段階が区別され、mapper-first API は入力型を先行推論しますが、実行時の引数受け渡し順序は変更しません。
`llvm_parallel.rs` は 4096 要素を目標値とし、最大 1024 チャンクに分割する計算式と `void(ptr, i64)` コールバックを生成し、不要な Task 配列を作ることなく同一のランタイムを効率的に呼び出します。
副作用のない非消費コールバックであることが証明されている場合は既存の特殊化関数を直接呼び出し、それ以外の場合はチャンクごとに保持したスナップショットと完全適用ごとのクローンを使用します。
入力データや初期値（identity）は必要な箇所でのみ `clone_value` され、結果メモリ領域はワーカースレッドの起動前に一括確保されます。リダクション処理における最終結合はチャンクのインデックス順序に厳密に従って行われます。
アイドル状態のワーカーは条件変数で効率的に待機し、プロセス終了時の `atexit` ハンドラがワーカーの停止、起床、およびスレッドジョインを安全に行います。pthread、同期プリミティブ、または atexit の呼び出しが失敗した場合は、標準エラー出力に詳細を出力して即座にアボートします。
WASM 環境では同一のコールバック関数を添字順に逐次呼び出し、ホスト環境のスレッド機能を要求しません。
独立したタスク同士の実行順序は言語仕様上未規定ですが、各タスク内部における評価順序、および結果配列内の要素順序は完全に保持されます。

ネイティブ並列ランタイムの外部シンボル名は `tsuzuri_task_parallel` と定義され、公開 ABI のプレフィックス `tz_` との衝突を防止しています。
ドライバーは並列処理が必要とされた場合にのみ同梱の C ランタイムコードをコンパイルし、実行ファイルの生成時には `-pthread` を付与してリンクし、オブジェクトファイルの生成時には再配置可能リンク（relocatable link）によって内部に組み込みます。ランタイムのエントリーシンボルは weak かつ hidden 属性となっており、複数の生成オブジェクトを同時にリンクした場合でも単一の実体に安全に統合されます。
macOS における再配置可能リンクでは `-keep_private_externs` を指定し、中間リンクの段階で weak／hidden なシンボルが意図せずローカルシンボルへと固定化されてしまうのを防ぎます。
単一シンボルの整合性、および 2 つのオブジェクトを同時実行した場合でも合計ワーカー数が上限値内に収まることがテストされています。
生のネイティブ LLVM IR を外部で直接リンクして使用する開発者は、`src/runtime/task.c` をコンパイル対象に含め、リンカーに `-pthread` を指定してください。
なお、LLVM IR や C ヘッダーの出力、および Cargo によるコンパイラのビルドにおいては、引き続きホスト環境の LLVM や pthread 開発用ヘッダーを要求することはありません。

**所有権:** 型検査済みのローカル変数 ID および構造体のフィールドパスに対して、move（移動）と loan（借用）の妥当性が厳密に検証されます。
関数引数などの一時的な借用であっても、後続のオペランド式の評価がすべて完了するまで安全に保持されます。
再借用（reborrow）は元の借用を親として階層的に追跡され、子借用が生存している間は元の排他参照を使用したり move したりすることはできません。
関数の戻り値や外側のスコープの束縛へ参照を返却・代入する際は、参照先のリソース所有者がそのスコープを超えて生存し続けることが静的に検証されます。
共有借用フィールドを持つ構造体の定義が許可されており、格納型の走査では record、union、コレクションを通じて参照型および排他参照型の存在が検査されます。借用の省略表記においては、入力引数の借用集合の積集合（intersection）が正しく計算・保持されます。
`TypeExprKind::Regions` および関数宣言に付与された region リストは `regions.rs` によって検証され、関数の返却契約が `CheckedFunction.region_sources` に記録されます。
この契約は、戻り値の各 region スロットについて「どの引数インデックスのどのスロットが借用元になり得るか」のペア情報を保持します（`RegionSources`）。複数の region を持つ多 region レコードは、`CheckedRecord.field_regions` においてフィールドごとの region 所属を表す `RegionMask`（`u16`、最大 16 個）を保持します。
所有権チェッカーの `Value` 構造体は、多 region レコードの値についてのみスロットごとの借用集合を正確に保持し、レコードリテラル、レコード更新、フィールド読み出し、不変変数束縛、`if`／`match` の分岐合流、および直接の完全適用呼び出しにおいて各スロットの独立性を維持します。
それ以外の複雑な経路ではスロット情報を統合して全借用として保守的に扱います。所有権検査は、戻り値の借用の外部ルートが関数の契約に含まれていることを確認し、直接の完全適用呼び出しにおいてのみ指定された入力引数の借用を戻り値へと正確に伝播させます。
名前付き関数の引数に指定された region 全称量化関数型（`TypeExprKind::Quantified`）は、関数単位のコールバック契約 `CallbackContract` を形成します。
関数本体においてその引数を完全適用した結果は、契約で定められた入力引数の借用のみを保持し、呼び出し側は引き渡す関数（名前付き関数の契約、ラムダ式の本体、同一契約を持つ引数）がこの契約を厳格に遵守しているかを検証します。
この量化関数は直接の完全適用呼び出しでのみ使用が許可され（`regions::validate_contract_calls`）、契約情報が一般の関数値へ不正に漏洩することを防ぎます。
通常の `Type` や LLVM コード生成からは region や量化情報は完全に消去され、明示的な量化を持たない関数値や部分適用では、すべての入力に対する保守的な借用追跡が維持されます。
排他借用フィールド（A13）は `regions::exclusive_field_regions` が宣言を検査し（region 必須・排他 region の位置の一意性・入れ子への region 適用）、`check::mark_exclusive_regions` の固定点が `CheckedRecord.exclusive_regions` と、フィールドごとの参照先の region（`field_targets`・`region_targets`）を計算します。
排他借用への経路はレコードの直接のフィールドと、それを入れ子にしたレコードのフィールドだけに限られ、配列・タプル・union・共有参照の内側や型変数の具体化は宣言・具体化の検査で拒否されます。`Type`・`Loan`・`Value`・`Place` の表現は変えていません。
フィールド経由の再借用は既存の `Borrow(Dereference(Field(..)))` と loan の親で表し、引数の外部 loan はスロットごとに、排他 region のスロットと `ref mut` 引数だけを可変にします。書き込みの可否はその場所に重なる経路の loan だけで判定し、共有参照を経由した排他アクセスは `shared_exclusive` が `E1014` で拒否します。
参照先の region を書いた参照（`ref mut {r} T {s}`）は、参照自身と参照先の 2 スロットを持ちます（`CheckedFunction.region_slots`）。参照を経由した借用集約型の置換は、ローカルの所有者では新旧の借用を合わせて保持し（weak update）、引数の参照先では参照先の region を持つ入力の借用だけを受け付けます（`Checker::store_through`）。
参照先へ入力を格納しうる関数（`RegionSlots.writes`）の呼び出しでは、呼び出し側が格納される入力の借用を参照先の所有者へ加え、この関数は直接の完全適用だけが許可されます。
参照型を含む構文のパース処理は独立したヘルパー関数に分離されており、最大深度 128 の構文木においてもパーサーのスタック消費量が一定に保たれます。
レコード、配列、リストにおける Copy 特性は構造的に判定され、文字列を含むデータ構造の値は常に所有権の移動（move）となります。

**配列:** `Type::Array` は要素型のみを型情報として保持し、LLVM 上では `%tz.array = { ptr, i64 }`（データポインタと要素数）として表現されます。
`new [T](length, initializer)` および `new [...]` は所有ヒープ領域を動的に確保しますが、`new` を伴わずに変数に束縛された配列リテラルはスタックフレーム領域を使用します（後述の **記憶域** 参照）。いずれの形式で生成された場合であっても、構築完了後に配列の長さや要素を変更することはできません。
初期化関数の式は構築開始時に 1 回だけ評価され、各インデックスに対してその関数値のスナップショットが適用されます。
メモリ確保に先立って、負の要素数、および要素サイズとの乗算における整数オーバーフローが厳格に検査されます。
ヒープに確保される空配列、およびサイズ 0 の要素型を持つ配列であっても、確保バイト数は最低 1 バイト以上が要求され、プラットフォーム依存の `malloc(0)` の挙動に依存しない安全な設計となっています。
`new` を伴わない空配列リテラルはメモリ実体を持たない `{ null, 0 }` として表現され、その解放処理は安全な no-op となります。
配列の Copy 操作やクロージャ環境の複製時には内部バッファが独立してディープコピーされ、要素型の clone や drop も反復して実行されます。
要素型がプリミティブなスカラー型であっても配列バッファ自体のメモリ解放が必要となるため、すべての配列型において `needs_drop` は真となります。
長さの取得、インデックス読み出し、およびスライス借用において配列全体が無駄に複製されることはなく、要素の読み出しに必要な最小限の複製と一時値の解放のみが行われます。
単相化や型クラスのインスタンス選択に配列の長さは関与しません。再帰 union の要素配列は、前述のスタックレス反復走査機構へと登録されます。

**連結リスト:** `Type::List` は構文上の `[|T|]` に対応し、LLVM 上ではリスト記述子 `%tz.list = { ptr, i64 }`（先頭ノードポインタと長さ）およびノード構造体 `{ ptr, T }`（次ノードポインタと要素値）として表現されます。
`new [|...|]` および `new [|T|](length, initializer)` は評価順序に従って各ノードをヒープ上に確保しますが、`new` を伴わずに束縛された `[|...|]` はスタックフレーム上の連続したノード配列を連結して構築されます。構築完了後にリストの内容を変更することはできません。
空リストの先頭ポインタは NULL であり、ノードのメモリ確保は発生しません。`.length` プロパティの取得は $O(1)$、添字アクセスは範囲検査の後にリンクを順次辿る動作となります。
構造的な Copy やクロージャ環境の複製時にはノードおよび要素が独立してディープコピーされ、すべてのリスト型において `needs_drop` は真となります。
clone、drop、および添字走査は反復ループによってスタックレスに処理され、デストラクタではノードを解放する前に次のノードポインタを確実に先読みします。
リスト構築用の一時スロットもエントリーブロックに配置され、非常に長いリストの生成や末尾再帰の実行時にもコールスタックを累積消費しないように最適化されています。
配列と同様の多相化、所有権、借用追跡、および ABI の制限が適用されます。

**記憶域（スタックとヒープ）:** `new [...]` や `new [|...|]` は構文 AST および型付き IR において `NewLiteral` でラップされますが、型および所有権の規則は内側のリテラルと完全に同一です。
値がスタックに配置されるかヒープに配置されるかは、型システムではなく LLVM emitter が値の生成コンテキストに基づいて静的に決定し、`Type`、型クラス、単相化、および ABI のレベルには現れません。
`let` の初期化式、`match` の対象式、`for` ループの列挙元、ならびに添字アクセス・長さ取得・文字列比較・文字列連結の被演算子に出現するリテラルの構文木（`if` や `match`、ブロック式の結果、およびレコード・タプル・コレクションのリテラル要素を経由したものを含む）は、`frame_value` によってスタック上に構築されます。
配列は `[N x T]`、リストは `[N x { ptr, T }]` のエントリーブロック `alloca`、文字列リテラルはプライベート定数領域を直接参照し、空のコレクションは `{ null, 0 }` となります。
各値には、その生成位置に対応するスタックフレーム領域の候補（`Frame`）が静的に紐付けられます。

スタックフレームのポインタは、「その値を受け取った束縛変数（または一時変数スロット）内部の静的アドレス」、および「関数呼び出し中のみ借用ワーカーに貸し出される引数」にのみ存在する、というのがコンパイラの厳格な不変条件です。
メモリ上の場所からの消費的な読み出し（move）はすべて `relocate` を経由し、関数の戻り値、実引数、クロージャ捕捉、再代入、別の変数束縛、構造体のフィールド、および末尾ループのバックエッジへ移動する値は、すべて自動的にヒープ領域へと移送されます。Copy 型の値の複製も、従来どおり `clone_value` がヒープ上に新しく作成します。
唯一の例外は、値を読み出した直後にその場で安全に破棄される短命なコンシューマ式です。文字列連結の被演算子、および借用ワーカー関数へ渡される一時的なクロージャ環境の捕捉値（`capture_values`）はスタックフレームのまま受け取ることが許可され、連結完了後または呼び出し完了後の解放処理においてフレームの存在が適切に考慮されます。
借用ワーカーはプレフィックス引数を move や drop しないことが特殊化の前提条件となっているため、呼び出し中もスタックフレームの生存期間内に安全に収まります。
`ref mut` による可変借用を確立する前には、対象の場所の値を事前にヒープへと移送してから貸し出すため、借用先での値の置換や drop によってスタックフレームが不正に解放されることはありません。
共有借用、インデックスアクセス、長さ取得、`for` ループ走査、およびパターンマッチの読み取り専用別名はメモリ移動を伴わず、所有権チェッカーがその参照の生存期間を束縛スコープ内に限定します。
実行時において、値が依然としてスタックフレーム領域を使用しているか否かは、データポインタ（または先頭ポインタ）と長さ情報の双方を比較することによって決定論的に判定されます。
これにより、部分 move 後のゼロクリア、`let mut` への再代入、および分岐の合流点であっても、複雑な静的状態追跡を行うことなく安全に処理でき、長さの比較によって「移動後のゼロ」と「WASM のアドレス 0 に配置された alloca」とを確実に区別できます。
フレーム領域を使用している値の drop 処理では、要素が保持する所有値のみを再帰的に解放し、スタック領域自体の `free` は行いません。ヒープへの移送時には要素データをビット単位でコピーし、入れ子になった候補値も再帰的にヒープへ移送します。
配列やリストの要素は不変であり、要素単体を取り出して move することはできないため、親コレクションがフレーム上に存在する限り、入れ子の要素も元のスタックフレームを安全に参照し続けます。
単一のリテラル木のスタックフレーム領域が保守的な見積もりで 64 KiB（`MAX_VALUE_BYTES`）を超過する場合は、スタック溢れを防止するため木全体を最初からヒープ上に生成します。
変数に束縛されず直接返却される戻り値や実引数の位置に書かれたリテラルは、無駄なスタック配置と移送をスキップして直接ヒープ上に構築されます。
なお、関数値のクロージャ環境およびタスクはこのフレーム最適化の対象外であり、既存の特殊化機構（既知のエスケープしない呼び出しに対するスタック環境割り当て）が適用されます。

**コレクションの不変性:** 配列およびリストの各要素に対する再代入や可変借用（`ref mut`）は言語仕様として禁止されています。
要素型から入れ子のフィールドや共有参照を辿って可変参照に到達できるような型定義も `validate_size` によって厳格に拒否され、単相化後のすべての具象型に対しても再検査が行われます（なお、関数シグネチャに含まれる可変参照はデータを格納する状態ではないため対象外であり、排他参照のクロージャ捕捉は既存の Capture 検査で拒否されます）。共有借用された要素の生存期間は通常の借用（loan）追跡によって安全に保護されます。
可変変数（`let mut`）や `&mut` 参照を通じてコレクション全体を丸ごと新しいコレクションで置き換える操作は、従来どおり許可されます。

**LLVM:** 整数の算術命令に `nsw`（No Signed Wrap）や `nuw`（No Unsigned Wrap）の未定義動作フラグを付与することはありません。除算および剰余算ではゼロ除算と符号付きの `MIN / -1` を明示的に検査し、ビットシフト演算ではシフト量をビット幅に合わせてマスク処理し、浮動小数点数から整数への変換には飽和変換 intrinsic（`llvm.fptosi.sat`／`llvm.fptoui.sat`）を用いて poison や範囲外による未定義動作を徹底的に排除します。
`@checked` ブロック内部に記述された整数の `+`、`-`、`*`、`**`、および単項 `-` のみは、`Int.checked_*` と同一の `checked_integer_arithmetic` によってオーバーフローが検査され、オーバーフロー時には `OverflowException` を送出します。整数のべき乗 `**` は `Int.wrapping_pow` と同様に二乗法と乗算の反復（符号付きの負の指数は `NumericRuntime` のトラップ）で計算され、`f32`／`f64` の `**` は `Math.pow` と同一の `tz_math_pow_f*` を呼び出します。
例外は 32-bit 整数のエラーコードとして表現され、同一関数本体内の最も内側に位置する `try` のハンドラへと、途中のスコープの一時値を安全に解放しながら分岐します（`try_targets`）。
例外が関数やラムダ式の境界を越えて自動伝播することはありません。外側を囲む `try` ブロックが存在しない場合は `TrapKind::Overflow` として安全にトラップします。`finally` ブロックは、ハンドラ内部から送出された例外を含むすべての実行経路で確実に 1 回実行されます。
i8〜i64／i8u〜i64u から f32／f64 への変換には LLVM の `sitofp`／`uitofp` を用い、f32 と f64 の相互変換には `fpext`／`fptrunc` を使用します。i128、f16、f128、および decimal 型を含む変換は正確なソフトウェア演算経路を維持し、f64 を経由することで二重丸め誤差を生じさせることはありません。
intrinsic の外部宣言は組み込み関数と通常の式で共通化され、重複なく決定論的な順序で出力されます。
負の添字アクセスを含め、配列やリストの境界検査は要素の GEP／load やリンク走査よりも前に先行して実行されます。
浮動小数点演算に対して fast-math フラグを安易に付与することはありません。
借用可能な値のメモリ領域は関数のエントリーブロックに集約して配置されます。所有値を move すると移動元のメモリ領域は即座にゼロクリアされ、スコープ終了時に残存する文字列、配列、リスト、およびクロージャ環境が確実に解放されます（レコードの部分 move も同一の機構に従います）。
スタックフレームの領域を指している可能性のあるスロットの解放や、再代入前の解放処理においては、前述のアドレス判定ロジックによりフレーム領域を誤って `free` しないよう保護されています。

**末尾再帰:** 直接の自己末尾呼び出し（self tail call）は、実引数の評価が完了した後にループの先頭へと分岐し、すべての引数を phi ノードのバックエッジによって同時に更新します。
ネイティブ環境では、引数式に含まれる整数の `+` や `-` について、被演算子を元の記述順序（左から右）で評価して SSA 値に読み出した後、演算命令そのものを「全引数の評価完了後かつ drop 処理の前」の更新ステップへと遅延配置します。
これにより、ループカウンタの更新処理を本体のアルゴリズム計算と命令レベルで分離し、LLVM が `while` ループと同等の最適な命令選択（加算と条件分岐の融合など）を行えるように支援します。
遅延配置されるのは、副作用もトラップも発生しない固定ビット幅の折り返し加減算のみに限定されます。
オペランドの load、関数呼び出し、トラップ判定、および一時所有値の解放順序は一切動かさず、後続の引数が可変ローカル変数を変更した場合でも、先に確定した SSA 値がそのまま使われます。終了条件の判定、負数の挙動、overflow フラグ、浮動小数点演算の順序が改変されることはありません。
本体が引数同士の 1 つの演算のみで構成される既知の 2 引数関数に対しても同一の最適化経路が適用され、`Add.add`／`Sub.sub` の呼び出しと演算子記述の間でコード生成の差を生じさせません（内部に検査や追加処理を持つ関数は通常の呼び出しを維持します）。
WASM 環境においては、スタックマシン向けの従来の引数生成順序がそのまま維持されます。
双方の順序において同一の言語意味論が保たれているかを厳密にテストし、ネイティブ環境で効果的だった命令配置を特性の異なる実行基盤へ無条件に適用することはありません。
所有ローカル変数、配列アクセス、数値演算の一時 `alloca`、およびスタック上のリテラル領域は、必ず関数のエントリーブロックに配置されます。
これらをループの内部で都度確保すると、ソースコード上は末尾再帰であっても実行時にスタックを消費し続けてしまうためです。
ループ反復ごとに同一のリテラル領域を再初期化する場合でも、前回の反復で使用された値はバックエッジの前に解放済みであるかヒープへ移送済みです。
反復を開始する前に未移動の所有値は確実に解放されます。なお、借用を含む引数が存在する場合はフレーム領域の再利用は行われません。

**WASM:** bulk-memory 機能を標準で有効化し、集約値のメモリコピーにおいてホスト環境の `memcpy` インポートを要求しません。
LLVM の閉形式ループ最適化（closed-form loop optimization）は、`i64` のプログラムに対しても内部的に `i128` の乗除算命令を導入することがあります。
同梱のランタイムでは、乗算を i64／32-bit limb、除算を固定ビットシフトと減算の組み合わせで自前実装し、その補助関数自身が wide multiply/divide の libcall 呼び出しへと再帰的に再変換されて無限ループに陥る循環を防止しています。
LLVM 23 は limb の乗算式を再び i128 乗算としてパターン認識してしまうため、乗算補助関数には `noinline optnone` 属性を付与して InstCombine パスから保護しています（可変シフトの limb 補助関数も同様に保護されています）。
なお、アプリケーション関数に対する通常のコード最適化は完全に維持されます。
weak かつ hidden な補助関数シンボルはオブジェクト間での重複が許容され、未使用のものは `wasm-ld` のデッドコード削除によって最終バイナリからきれいに除去されます。
WASM ヒープマネージャはアドレス順に整列された空き領域リストを管理し、メモリの分割および隣接する空き領域の自動結合を行います。
`memory.grow` の失敗やメモリ上限（デフォルト 16 MiB）の超過時には安全にトラップを発生させ、見かけ上成功したかのような無効ポインタを返却することはありません。
ヒープ上限およびメインスタックサイズは、`BuildOptions` および `TestOptions` の `wasm_max_memory` と `wasm_stack_size`（CLI オプション `--wasm-max-memory`、`--wasm-stack-size`）によって設定可能です。
プロジェクト読み込み後、未指定のオプションはルートパッケージのマニフェスト内 `[wasm]` セクション（`Project::wasm`、`BuildOptions::with_manifest_wasm`）から補填され、適用後に妥当性が再検証されます。
`driver::wasm_memory_limits` がターゲットごとのメモリ範囲（wasm32 は 4 GiB − 64 KiB、wasm64 は 16 GiB）を一元的に検査して実効値を算出し、ビルドおよびテストランナーはその実効値から `wasm-ld` の `-z stack-size=` および `--max-memory=` 引数を組み立てます。
ヒープランタイム側の `heap-wasm.ll` は直接改変せず、`llvm::with_wasm_heap_limit` が生成後の IR 内にある `%fits` の 2 行と `%within` の 1 行を行全体の一致判定で安全に置換します。
ユーザー定義の文字列定数は `c"..."` の行の途中に出現するため影響を受けず、デフォルト値の環境では置換を行わないため、デフォルト設定時の IR および生成される `.wasm` バイナリは完全に不変です。
上限が 2 GiB 以下であれば `i32` の終端計算 `%begin + %needed` はオーバーフローしません。2 GiB を超過する場合は、`%within` を `%room = sub i32 LIMIT, %begin` との比較を行う 2 行の命令へと置き換えます（`%begin` は上限を超えません）。
上限を 4 GiB − 64 KiB に制限しているのは、アライメント計算 `%ceil = add i32 %end, 65535` がオーバーフローするのを防ぐためです。JavaScript の同梱ホストはポインタを符号なし 32-bit 整数（`>>> 0`）として受領します。
wasm64 ターゲットでは `heap-wasm64.ll`（ヘッダーは i64 の capacity および next、`memory.size`/`grow` は i64）を使用し、同様の 3 行置換によってメモリ上限を設定します。
`Instrumentation::memory64` は、ホスト ABI のメモリ範囲検査、スカラー捕捉のビット幅、および DWARF デバッグ情報のポインタ幅をすべて 64-bit に設定し、Clang には `--target=wasm64-unknown-unknown`、`wasm-ld` には `-mwasm64` を指定します（なお、スレッド機能 threads は現在のところ wasm32 のみ対応です）。
スタックは枯渇するとアドレス 0 を下回って循環（ラップアラウンド）します。wasm32 で 2 GiB を超えるメモリ空間や、ワーカースレッドのスタックをヒープブロック上に確保する threads 環境では、ラップアラウンド先が正常なヒープメモリ領域と重なってメモリ破壊を引き起こす恐れがあります。これを防ぐため、`llvm::with_stack_checks` が全関数のエントリーブロックにおける静的 alloca の直後にスタック境界検査 `@tz.stack.check` を自動挿入します。
alloca 命令を関数呼び出しより前に集約配置するのは、インライン展開によってエントリーブロックが分割された際に動的 alloca（ループ反復ごとにスタックが増大し続け、SROA 最適化も阻害される現象）化するのを防ぐためです。
スタック検査は、`llvm.frameaddress(0)`（プロローグ直後の副作用のないスタックポインタ値）を許容範囲 `[__stack_low + 4096, __stack_high]` と比較し、範囲を逸脱した場合は即座に `llvm.trap` を実行します（`llvm.stacksave` は副作用属性を持つため LICM やベクトル化を阻害しますが、`frameaddress` は安全に最適化を維持できます）。
この検査はトラップ情報の計測処理よりも前に挿入されるため、`@tz.stack.check` もコンテキスト ID を受け取ることができ、スタック溢れを `stack overflow` として正確に診断報告します。計測処理が後から付与するレポーター関数や `tsuzuri_trap_site` 自身はスタック検査を持たないため、スタックポインタが不正な状態のままでも安全に呼び出すことが可能です。
threads 環境では、WASM グローバル変数 `tsuzuri_stack_base` および `tsuzuri_stack_top`（インスタンスごとに設定、`!invariant.load`）が 0 であればメインスレッド、非ゼロであればワーカースレッドのスタック範囲として判定します。4096 バイトの安全マージンは、スタック検査を持たない `task-wasm-threads.c` および `wasm.ll` のフレームサイズの合計（`-O0` で 640 バイト）を十分に包含する設計となっています。
threads 有効時はインポートセクションの最大メモリサイズも同一の値となり、`--emit object` ではヒープ定数のみを保持します。ビルドキャッシュキーにはオプション設定と置換後の IR が含まれるため、上限値の変更も正確に反映されます。
検証は、`tests/wasm_memory.mjs` による `-O0`／`-O3` でのデフォルト出力不変性、2 GiB 超過時の room 形式と境界動作、スタック検査、マニフェスト設定、診断メッセージの検査、`tests/wasm_threads.mjs` によるワーカーのスタック溢れが隣接ブロックを破壊しないことの確認、ならびに `tests/wasm64.mjs`（Node.js 24）による wasm64 の上限・境界動作・4 GiB 超のホストバッファ・テストスイートの実行確認によって行われています。

**ホスト境界:** Tsuzuri の関数内部に外部の C/C++ 関数を直接インポートして実行することはできません。
コンソール出力用の `putchar` は、コンパイラが自動生成したエントリーラッパー関数のみが保持します。数値演算が外部の `printf` に依存することはありません。
公開 ABI は、従来のスカラー型に加え、共有借用の i64／f64／ubyte 配列、両文字列型（`string`、`utf8string`）の入力、所有権付きバッファの戻り値、およびスカラー型のみで構成されるレコード型を安全に取り扱うことができます。
`abi.rs` が型、データの受け渡し方向、およびレコードのフィールド名を統一的に分類し、`llvm_abi.rs` が C ヘッダーとラッパー関数を生成します。
バッファ入力はポインタと長さから直接記述子を構築し、共有配列は SSA 値として、文字列はスタック記述子への参照として渡されます。
`string` は 16-bit の UTF-16 コード単位、`utf8string` は 8-bit の UTF-8 バイト列です。UTF-8 のバリデーションにはデコード失敗を値として返す既存の内部デコーダを用い、入力データを余分に確保・変換することはありません。
レコード型は専用の ABI 構造体と内部表現の間で再帰的に相互変換され、`bool` や狭幅整数は 32-bit に正規化されます。関数の戻り値は出力先ポインタ（out pointer）に書き込まれ、バッファの所有権のみがホスト側へと安全に引き渡されます。
メモリアロケータは、拡張 ABI の使用時に限り弱いシンボル（weak）として `tsuzuri_alloc` および `tsuzuri_free` を公開し、ネイティブではシステムの `malloc`／`free`、WASM では既存の WASM ヒープを使用します。
POSIX ネイティブ環境のアロケータは、共通のフックテーブルから境界ランタイムの存在を検知し、メモリの確保と解放を安全な追跡経路へとルーティングします。これは拡張 ABI を使用するデフォルトネイティブ IR における内部変更ですが、公開シグネチャそのものには一切影響しません。WASM および Windows におけるデフォルトアロケータは従来の動作を維持します。
**allocator の選択（F13）:** heap runtime は `emit_program` の末尾で一つだけ連結し、`@tz.alloc`／`@tz.free`／`@tz.realloc` を `define internal` で定義します（呼び出し側は allocator を知りません）。`llvm::Allocator::System` は従来の `heap-native.ll`・`heap-wasm.ll`・`heap-wasm64.ll`（threads は lock 版）、`Host` は `heap-host.ll`（WASM では `tsuzuri_heap` からの import）、`Counting` は対象の heap を `counted_base` で `@tz.alloc.base` などへ改名し、その上に `heap-counting.ll` を置きます。
`Host` と `Counting` は各ブロックの先頭 16 バイトに要求サイズを書き、ホストへ渡すサイズと統計を解放の経路によらず一致させます。トラップの理由は関数名で分類する（`traps::runtime_kind`）ので、確保の失敗の `@llvm.trap` は `@tz.alloc`・`@tz.realloc` とその `.base` の本体に置きます。
`--freestanding` は `--allocator host` に加えて CPU ディスパッチを使わない経路（`emit_native_build` を通らない）で出力し、IR が C ライブラリを要する runtime（IO・OS・タスク・引数・`write`）を宣言したら `E2000` にします。`--emit header` の出力には IR がないので、同じ build の object が持つ library の IR を別に生成して検査します。
128-bit 値、ソフトウェア浮動小数点型、任意の所有入力、借用参照の戻り値、およびクロージャ環境の直接的な ABI 公開はサポートされていません。
外部シンボルのインポートはユーザーが記述した `extern` 宣言からのみ発生し、リンク名、ハンドル型、コールバックを使用しないプログラムにおいては、生成される IR、C ヘッダー、および WASM インポートの構造に変化はありません。
生成バインディング（E13）と `--emit shared` は IR・WASM・C ヘッダーを変えず、既存の export（`memory`、`tsuzuri_alloc`、`tsuzuri_free`、`tz_*`、`tsuzuri_trap_site`、`__indirect_function_table`）だけを使います。グルーはモジュールが宣言した import だけを渡して import を足さず、threads・IO・Debug・WASI・host allocator のモジュールは明示的な例外で拒否します。record の offset は `record_layout` と同じ規則で求め、`export def` の `ref H` 引数は、非拡張の wrapper でもハンドルを slot へ置いてから借用として渡します。
外部ライブラリのリンク入力はネイティブ実行ファイルのビルドでのみ有効です。`wasm-ld` の `--export-table` はコールバックラッパーが存在する場合にのみ渡され、変数を捕捉した関数値が ABI を越えて直接渡されることはありません。
GUI、ユーザー入力イベント、ネットワーク通信、非同期 I/O、およびイベントループは、ホスト環境との境界で適切に取り扱われます。ファイル操作、環境変数、システム時刻、乱数生成、および子プロセス起動は標準ライブラリの OS API が安全に仲介し、ユーザー定義の `extern` や公開 ABI を無秩序に増やすことはありません。

**出力保護:** コンパイラは入力全体の型検査が正常に完了した後、出力先と同一のファイルシステム上に作成した専用の一時ディレクトリ内でビルド処理を行います。
すべての関連ツールの実行が成功した後にのみ、アトミックなリネーム（rename）によって最終成果物を公開配置します。
この出力保護機構はルートパッケージのファイルだけでなく、プロジェクトに読み込まれたすべてのソースファイルに対して適用され、Unix 系 OS ではソースファイルへのハードリンクの作成も厳しく拒否されます。
他のプラットフォームにおいても、アトミックな置換処理によって別のハードリンク先の内容が意図せず書き換えられる事故を防ぎます。
出力ファイルを事前に `truncate`（切り詰め）して空にすることは決して行いません。

## 開発と検証

コンパイラの健全性を検証するための標準的なコマンドライン手順は以下のとおりです。

```sh
sh scripts/check-runtime-includes.sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
node tests/e2e.mjs target/release/tsuzuri
node tests/primitives.mjs target/release/tsuzuri
node tests/tasks.mjs target/release/tsuzuri
node tests/computations.mjs target/release/tsuzuri
node tests/control.mjs target/release/tsuzuri
node tests/numeric_casts.mjs target/release/tsuzuri
node tests/integer_intrinsics.mjs target/release/tsuzuri
node tests/display_parse.mjs target/release/tsuzuri
node tests/json.mjs target/release/tsuzuri
node tests/os.mjs target/release/tsuzuri
node tests/repl.mjs target/release/tsuzuri
node tests/script.mjs target/release/tsuzuri
node tests/examples.mjs target/release/tsuzuri
node tests/features.mjs target/release/tsuzuri
node tests/packages.mjs target/release/tsuzuri
node tests/wasm_memory.mjs target/release/tsuzuri
node tests/bindgen.mjs target/release/tsuzuri
node tests/bindings.mjs target/release/tsuzuri
node tests/bindings_threads.mjs target/release/tsuzuri
node tests/host_bindings.mjs target/release/tsuzuri
npx --yes --package=node@24 node tests/wasm64.mjs target/release/tsuzuri
```

Rust レベルの単体テストは、LLVM がインストールされていない軽量環境でも完全に実行可能です。字句解析、型検査、エラー発生ケース、メモリレイアウト計算、IR の不変条件検証、および 6,500 パターンに及ぶ決定論的なソースコード変異テスト（mutation test）を網羅的に実行します。
`tests/diagnostics.rs` および CLI E2E テストは、複数ファイル構成、宣言境界の判定、括弧内行頭の `let`、不正なシグネチャや認識器のエラー抑制、関数内部での型・所有権エラーからの回復、スコープの復元、エラー関数から返された参照による二次エラーの抑制、50 件を超える正確な省略件数表示と 1000 件の収集上限、デフォルトスタックでの深い回復処理、JSON lines 形式の出力、ならびにファイル出力保護の堅牢性を検証します。
所有権テストでは、正常な move、共有借用、排他借用の検査に加え、move 後の不正使用、部分 move、条件分岐や短絡評価における追跡、再借用の整合性、生存期間の満了検出、およびオペランド評価中における参照先の早期無効化を厳格に検出します。
Node.js による E2E テストスイートは、本物の Clang／LLD ツールチェーン、ネイティブ C ホスト環境、および WebAssembly 実行エンジンを用い、JavaScript の `BigInt` や `Number` による独立した参照計算結果とビット単位で照合します。
最適化の無効化（`-O0`）と有効化（`-O3`）の双方でテストを実行し、100 万回の末尾再帰実行、トラップ発生の正確性、キャリーフラグを含む 128-bit 幅の補助関数の挙動を網羅します。外部ツールが存在しない場合にテストを勝手にスキップして成功扱いにすることはありません。
サンプルコードの検証には、HTTP 経由での WASM モジュール読み込みや、Python によるデスクトップホストのヘッドレス実行環境も含まれます。
複数ファイルのテストでは、モジュール間の名前空間の分離、修飾された高階関数およびレコードの呼び出し、相互再帰、`Main.tz` の選定ロジック、ファイルごとの診断表示、および依存ソースの出力保護が検査されます。
`tests/polymorphism.rs` は、抽象関数本体、型クラス制約、関数要件制約、インスタンスの重複判定、特殊化処理、および型の再帰的増大の検出を検証します。
`tests/e2e.mjs` の多相性 fixture は、C および WASM において整数、浮動小数点、ユーザー定義レコードの演算、高階関数、型クラスのメソッド値、型に基づくモジュール関数の選択と部分適用、所有文字列、評価順序、および 100 万回の末尾再帰を実行して正確性を担保します。
`tests/primitives.mjs` は、decimal 演算の結果を Python の IEEE 754 準拠 decimal コンテキストと厳密に照合し、ネイティブ環境でのメモリ確保と解放を追跡してメモリリークや二重解放を検出します。
`tests/numeric_casts.mjs` は、全ビット幅の整数・符号と f32／f64 間の高速型変換を、`BigInt` による直接丸め、飽和演算の参照実装、および f128 を経由する正確なソフトウェア実装と照合します。NaN、無限大、符号付きゼロ、非正規化数、丸めの中点（tie）および二重丸めが発生しやすい境界値、ならびに飽和の限界値を、ネイティブおよび WASM の `-O0`／`-O3`、ならびに native CPU 指定の各環境で検証します。
`tests/display_parse.mjs` は、すべての f16 ビット列、f32／f64 各 10,000 パターン、f128 の 2,000 パターンに及ぶ決定論的なランダム列と境界値、ならびに decimal や全整数幅の値を、Python の `Fraction` を用いた区間内整数仮数探索および `Decimal` の独立リファレンスと照合します。最短桁表示、非 NaN におけるビット往復の完全性、NaN の分類、decimal の数値および符号付きゼロ、解析失敗時のエラー処理、リソース上限の挙動を、ネイティブおよび WASM の `-O0`／`-O3` で検査します。
`tests/json.mjs` は、RFC 8259 の例、すべてのエスケープ、孤立サロゲート、生の制御文字、先頭ゼロや末尾カンマなどの拒否例、128／129 段の入れ子、32 メンバーを超える object の重複キー、seed 固定の LCG で JavaScript が作る 200 個の値（詰めた形と字下げした形）からなる約 300 個の入力を `Cases.tz` に生成し、`Json.parse` の受理・拒否（種類とバイト位置）と `Json.to_utf8string` の FNV-1a を、`JSON.parse`／`JSON.stringify` と手で求めた期待値（字句を保つ数値、整数形のキーの順）に照合します。ネイティブ（確保の追跡で `live == 0`）と WASM（import なし）の `-O0`／`-O3` で実行します。
`tests/strings.rs` および `tests/strings.mjs` は、UTF-16 の型表現、サロゲートペア、文字エンコーディング変換、および従来の UTF-8 動作を検証します。Node.js の標準 `String` をリファレンスとし、コード単位数、添字アクセス、文字列比較をネイティブおよび WASM の各最適化レベルで照合します。
同一のランナーが `tests/strings_runtime.c` の独立した整数演算リファレンスを用いてすべての Unicode スカラー値を小分けに往復変換し、不正な UTF-8 シーケンスや孤立サロゲートの変換が確実にトラップされることを確認します。
また、巨大メモリを確保しないシミュレーション用アロケータにより、$2^{53} - 1$ の文字長上限およびコード単位あたり 2 バイトのメモリ消費量を照合します。WASM においては累積確保量がメモリ上限を超えるような反復処理を実行し、解放された空き領域が正しく再利用されることを検証します。
カリー化のテストでは `tests/currying.rs` が型、捕捉、生存期間を検証し、`tests/fixtures/currying` を `tests/primitives.mjs` から C および WASM の双方で実行します。匿名関数、途中段階での副作用、関数を含む集約値、入れ子の所有文字列、ならびに 40,000 回に及ぶ環境の生成・複製・解放を追跡し、公開関数の各呼び出し完了後にネイティブメモリの未解放バイト数が完全に 0 に復帰することを確認します。
配列のテストでは `tests/arrays.rs` と `tests/fixtures/arrays` を用い、型情報から配列長が独立していること、実行時の動的生成、異なる長さを持つ入れ子配列、空配列、不変性、および借用参照の生存期間を検証します。`tests/primitives.mjs` では 1024 要素を超える生成、確保サイズ計算時のオーバーフロー検出、所有要素や関数要素の複製と解放、100,000 回の末尾再帰、ならびにトラップ挙動を確認します。
連結リストのテストでは `tests/lists.rs` と `tests/fixtures/lists` を用い、配列と同様の検証項目に加え、区切り記号と演算子の共存、型クラスの適用、配列との混在、および可変参照による不変性の迂回防止を検証します。`tests/primitives.mjs` は、ノードの確保数、100,000 ノードのスタックレス反復解放、100,000 回の末尾再帰、複製および所有要素・クロージャ環境の解放、ならびに WASM のヒープ再利用を検証します。
記憶域のテストでは `tests/storage.rs` が `new` 構文のパース、束縛変数や一時値リテラルのエントリーブロック `alloca` 配置、`new` やメモリ移動・`&mut` に伴うヒープ確保の位置、64 KiB 上限の境界判定、および末尾ループのエントリー配置を LLVM IR レベルで検査します。`tests/fixtures/storage` は `tests/primitives.mjs` からネイティブおよび WASM の `-O0`／`-O3` で実行され、ネイティブでは各呼び出しにおけるヒープ確保回数（スタックに束縛されたリテラルは 0 回、`new` やスコープ外への移動・Copy による複製は各 1 回以上）とメモリリーク 0 を厳密に照合します。

数値ランタイムを変更する際は、`src/runtime/numeric.c` を編集した後に `python3 src/runtime/generate.py` を実行して `numeric.ll` を再生成します。
続けて `python3 src/runtime/generate_math.py` を実行して `math.ll` も再生成し、メタデータおよび attribute ID が `numeric.ll` の最大値より後方へ割り振られるようにします。
`Math` モジュールを使用する場合にのみ `numeric.ll` の後に `math.ll` が結合され、重複する LLVM intrinsic 宣言が安全に除去されます。
トラップおよびループ用の生成メタデータ ID は、両ランタイムの最大 ID より後方の範囲に予約されます。
math 生成スクリプトは、必要な musl 1.2.5 のソースコードを個別にコンパイルしたうえで `llvm-link` を用いて結合します。
検証済みのビルド環境は Apple Clang 21 および llvm-link 21 であり、環境変数 `TSUZURI_CLANG` および `TSUZURI_LLVM_LINK` によってツールパスを指定可能です。
保存処理に先立って同一の Clang で IR のコンパイル検証を行い、未解決の外部関数参照、FMA 命令の暗黙生成、fast-math フラグ、および意図しない拡張精度の混入を厳格に拒否します。
通常の Cargo ビルドにおいて Clang や llvm-link への依存関係を追加することはありません。アップストリームのライセンス条項およびソース取得元のハッシュ情報は `runtime/musl` に同梱されています。

浮動小数点数の平方根（sqrt）および丸め処理は、f32／f64 では型付き intrinsic および明示命令を用い、その他の広幅形式では既存の整数数値ランタイムを使用します。
明示的な `Math.fma` のみが、AArch64 ネイティブ環境の f32／f64 において `llvm.fma` 命令へと lowering されます。ハードウェア命令を保証できないネイティブターゲット、WASM、およびその他の浮動小数点形式では、高精度なソフトウェア実装 `tz_soft_fma` が使用されます。
通常の乗算や加算に対して `contract` や fast-math 属性を付与することはなく、libm の `fma` インポートを無用に生成することもありません。
ソフトウェア FMA は、係数の積と加数を符号付き多倍長整数として正確に合成し、既存の `pack` 処理によって 1 回だけ丸め込みます。指数差が $3p + 8$ 桁を超える極小の非ゼロ項については、丸めの中点（tie）の判定方向のみを保存する sticky digit へと縮約します。
これにより、固定長の `LIMBS` 領域を無駄に拡大することなく、f128 や decimal128 の極端な指数差を持つ演算も正確に取り扱うことができます。特殊値（NaN、無限大など）は事前に処理され、decimal 型は十進表記のまま厳密に計算されます。
`Array` の `pairwise`、`Neumaier`、`dot`、`dot_fma` は通常の標準ライブラリ関数として実装されています。`pairwise` は 1 回の所有コピーを `Array.set` でインプレース更新し、各段の隣接ペア木構造を維持します。従来の `sum` や通常の内積計算は左結合の順序と個別の丸め処理を維持します。
ソフトウェア平方根 `soft sqrt` は目標の量子指数を算出した後、整数二乗比較によって仮数部を探索し、2 つの丸め候補の中点の二乗と比較して 1 回だけ正確に丸めます（decimal から binary への変換は行いません）。最小値・最大値関数（min/max）は NaN の伝播と符号付きゼロを厳格に扱い、クランプ関数（clamp）は引数の順序関係の検査を維持します。
初等超越関数（Elementary）は f32／f64 のみに提供されます。musl の標準的な f64 `atan2` 実装において 1 ulp（Unit in the Last Place）を超える誤差が確認されたため、有限の通常比の範囲については double-double 精度による範囲縮小と 31 次の多項式近似級数へと置き換えられました。
縮小後の変数値は 1/8 以下に収められ、多項式係数は 256-bit 精度から上位・下位に分割して保持されます（特殊値や極端な比率の処理はアップストリームの堅牢なロジックを維持しています）。
速度向上の誇大な主張は行いません。`tests/math.mjs` では、FMA を含む `BigInt` 参照実装による基本演算 759,675 件、および 256-bit `mpmath` 参照による初等超越関数 26,376 件をネイティブおよび WASM の `-O0`／`-O3` で照合検証しています。
f16 の単項基本演算は全 65,536 パターン、広幅形式は各 1,000 件の乱数パターンを含み、非 NaN におけるビット完全一致および実行時ヒープ非確保が確認されています。
機能テストの `fma_reductions` は長さ 0〜17 および 1025 までの境界値・奇数長において 872 ケースと 2 件のトラップを検証し、C 言語の `fma_runtime` テストはホスト環境の `fma`／`fmaf` と 20,513 組の演算結果を ASan／UBSan 有効環境で照合しています。

生成時のみ Clang を使用し、通常の Cargo ビルド、型検査、および LLVM IR の出力においてローカル環境への LLVM インストールを要求することはありません。
生成スクリプトは、ホスト環境のターゲットトリプル、データレイアウト、CPU 属性、および新しい IR 専用の属性を除き、little-endian ネイティブと wasm32 の双方で同一の整数アルゴリズムを使用します。
自動生成された IR ファイルを直接手動編集することは禁止されており、必ず C 言語ソースの変更とともにスクリプト経由で更新してください。
`numeric.ll` を含む `include_str!` の対象となるランタイム IR ファイル群はすべて Git のバージョン管理下に置かれます（`*.ll` の全体的な除外設定に対して `.gitignore` 内の例外指定で対応しています）。新しいランタイム `.ll` を埋め込む際は例外行を追加し、`sh scripts/check-runtime-includes.sh` を実行して追跡漏れがないことを確認してください。
再生成の整合性は、`python3 src/runtime/generate.py` の実行後に `git diff --exit-code -- src/runtime/numeric.ll` が差分なしで終了することによって検証されます。同梱の `numeric.ll` は Apple Clang 21 で生成されており、同一バージョンであればバイト単位で完全に一致します。
なお、LLVM 23 系の Clang はサイズ引数を持たない `llvm.lifetime.*` など LLVM 17 が解釈できない新しい形式の IR を出力するため、再生成には LLVM 17〜21 系の Clang を `TSUZURI_CLANG` で明示指定し、非互換な差分を手動で取り込まないように注意してください。

タスクの並行処理は、`tests/tasks.rs` が構文解析、単相化、move 挙動、捕捉された借用参照の排除、および決定論的 IR 生成を検証します。
`tests/tasks.mjs` は、ネイティブおよび WASM の `-O0`／`-O3` において逐次 bind、結果の順序性、入れ子になった並列処理、所有文字列・コレクション・関数値・タスク戻り値の安全性、数値境界、およびトラップ動作を確認します。
ネイティブ環境におけるヒープ追跡と、WASM のメモリ上限を超過する累積メモリ確保テストを通じて、未実行のまま破棄されたタスクを含む確実なリソース解放を検証します。
`tests/task_runtime.c` はハードウェア機能検出および pthread 呼び出しを計測用フックに差し替え、条件変数による実際の並行実行、共有プールの上限維持、全スレッドの完全ジョイン、逐次フォールバック、ならびにスレッド生成・ジョイン失敗時の適切な診断報告を検証します。
単なる経過時間の短縮をテストの合否判定基準とすることはせず、通常の関数呼び出しが完了した後にバックグラウンドで不要なワーカースレッドが残存しないことを厳格に検査します。

コンピュテーション式は、`tests/computations.rs` が拡張子ごとの宣言制限、複数型クラスの適用、ビルダーの各構文、単相化、クロージャ捕捉、生存期間、未実装操作の検出、ネスト展開深度、および未使用ビルダーの型検査を検証します。
`tests/computations.mjs` は、ネイティブおよび WASM の `-O0`／`-O3` においてカスタム短絡評価、複数回の yield、入れ子の反復、`Delay`／`Run` の有無による挙動差異、評価順序、数値境界、トラップ、およびタスクとの相互合成をテストします。
ネイティブ環境では全関数呼び出し後の未解放メモリバイト数が 0 であることを検証し、WASM 環境ではデフォルトの 16 MiB を超える累積確保の反復耐性を検査します。また、型注釈のない配列 bind と手書きのループ処理においてメモリ確保量が完全に一致することを確認します。
`.tt` および `.tc` におけるエラー診断の正確な位置特定、ファイル出力保護、ならびに決定論的な IR および WASM 生成も確認されます。
`tests/call_specialization.rs` と `tests/fixtures/computations/Optimization.tz` は、静的に既知の関数値、動的関数値、スコープ外へエスケープする関数値、可変引数の評価順序、所有権を持つ捕捉値および戻り値、相互再帰、ならびに特殊化予算上限に達した際の安全な通常経路へのフォールバックを網羅的に検査します。
コンピュテーション式の実行テストには、520 個の異なる継続によって予算を超過する大規模ケースや、`-O3` 最適化下で 262,144 回のコールバックを実行するストレステストが含まれ、ネイティブおよび WASM の双方でメモリ解放、スタック消費量、および演算結果の正確性が確認されています。
`benchmarks/run-computations.mjs` は、最適化後 IR におけるメモリ確保回数と、手書きの Tsuzuri および C++ 実装との実行時間を分離して測定し、共有 CI 上では `--quick` による意味論の正しさの検査のみを実行します。
Clang の AddressSanitizer（ASan）が利用可能な環境では、
`TSUZURI_ASAN=1 ASAN_OPTIONS=detect_stack_use_after_return=1 node tests/computations.mjs target/release/tsuzuri`
を実行することにより、生成されたネイティブ IR におけるヒープおよびスタックメモリアクセスの完全な安全性を検証できます。

制御構文のテストでは、`tests/control.rs` と `tests/control.mjs` が新しい構文、単相化、再帰の指定漏れ検出、move／loan の反復追跡、OR パターンの束縛とガード条件、アクティブパターンの評価順序と一時値解放、タプル処理、ならびに宣言およびパターンのネスト深度を検査します。
ネイティブおよび WASM の `-O0`／`-O3` において、整数の両端点、正負のステップ幅、ゼロステップ、空のコレクション列挙、NaN、符号付きゼロ、ソフトウェア数値演算、密なジャンプテーブルの境界外アクセス、ならびに 100 万回の match 末尾再帰を実行します。
ネイティブのメモリ確保追跡および WASM のヒープ再利用も検査され、
`TSUZURI_ASAN=1 ASAN_OPTIONS=detect_stack_use_after_return=1 node tests/control.mjs target/release/tsuzuri`
によって追加のメモリ安全性検証を実行可能です。
`benchmarks/run-control.mjs` は、同一の C 言語 ABI を共有する C、C++、Rust 実装と、match、if、`Sub.sub` による末尾再帰を含む 7 つの同一条件ワークロード（matched workloads）を比較します。
小規模な入力データについては独立した `BigInt` 参照実装とも照合され、共有 CI 環境では `--quick` によって意味論の正しさのみを検査します。
`--baseline` オプションを指定すれば、最適化適用前のコンパイラバイナリを同一プロセス内にリンクして交互測定を実施できます。
`tests/tail_recursion.rs` および `tests/fixtures/control/Recursion.tz` は、SSA 命令の配置順序に加え、負のカウンタ値、8／64／128-bit 整数のオーバーフロー折り返し、可変引数のスナップショット、左右オペランドの副作用、トラップ挙動、IEEE 754 加算順序、所有値の解放、ならびに借用を含む引数や非末尾呼び出しにおける安全な通常経路を厳格に検証します。

`tests/features.mjs` は、`tests/fixtures/<suite>` に配置された機能別のテストフィクスチャ（モジュールの可視性制御など）をネイティブおよび WASM の `-O0`／`-O3` で実行し、ネイティブのメモリ追跡によって各呼び出し後に全メモリが完全に解放されること、および WASM バイナリが不要な外部インポートを持たないことを検証します（第 2 引数にスイート名を渡すことで個別のテストスイートのみを実行可能です）。
例えば `node tests/features.mjs target/release/tsuzuri option_result` は、標準ビルダーにおける短絡評価、反復、部分関数のトラップ、参照経由のパターン束縛、Copy 値の独立性、および所有ペイロードの反復解放を包括的に検査します。

任意の実ブラウザ環境における検証手順:

```sh
TSUZURI_BROWSER="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
  node tests/examples.mjs target/release/tsuzuri
```

専用の一時プロファイルを動的に生成してヘッドレスモードの Google Chrome を起動し、WebAssembly モジュールの読み込み成功および UI の正常な活性化を確認します。ローカルマシン上に既存のブラウザプロファイルを使用することはありません。

新機能を設計・実装する際は、構文、型システム、LLVM コード生成、ホスト ABI、エラー診断、および関連ドキュメントのすべてを同時に同期して更新してください。
言語の基本的な値の意味論を改変してしまうような安易な最適化の導入は固く禁じられています。特に、短絡評価、トラップ挙動、符号付きゼロ、整数オーバーフロー、および評価順序の一貫性は、ネイティブと WebAssembly の両ターゲット環境において厳格に確認されなければなりません。
型付き IR が要求する重要な不変条件は、コード生成時における場当たり的なキャストによってではなく、必ずコンパイラフロントエンドの型検査によって論理的に保証される必要があります。

## 初版の次に必要な設計

共有可変キャプチャ（shared mutable capture）、外部パッケージレジストリからの自動依存解決、およびエフェクトシステムによる効果の型付けは現時点で未実装です。
例外処理は関数本体の内部における字句的な大域脱出のみに対応しており、関数の境界やラムダ式の境界を越えてスタックを巻き戻しながら伝播する機構は未対応です。
共有・排他の借用フィールドを含むレコード、複数 region の名前付き契約、再帰的なヒープ型、およびユーザー定義の `Drop` は実装済みですが、region 間の outlives 制約、`let mut` やループで合流する値の region ごとの追跡、トラップ発生時における安全なスタック巻き戻しと確実なリソース解放、ならびに汎用ホスト環境を跨いだ完全な所有権移転モデルは今後の課題です。
これらの新機能を追加設計する際にも、生存期間モデル、ホスト境界プロトコル、およびエラーハンドリングの失敗モデルを、型システムおよび静的検査と完全に統合して設計する必要があります。

標準 OS API、ハッシュコンテナ、および文字列補間は実装済みですが、Windows ネイティブの完全な OS API 対応（G10。現在は `E2002` エラー）、WASI preview2 および WebAssembly コンポーネントモデルへの対応（E13 の対象外で、計画チケットはありません）、ネットワークソケット API（E09）、ハッシュコンテナにおける SIMD を活用した群探査アルゴリズム（F08）、ならびに書式指定における Unicode 書記素クラスタ（grapheme cluster）幅の考慮（D09）は今後の実装課題として計画されています。
