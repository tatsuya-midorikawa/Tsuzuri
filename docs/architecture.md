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
   -> llvm -> reachability pruning of std code (a native executable emits only what its entry
              and exports reach) + bounded known-call specialization + deterministic LLVM IR
   -> Clang -O0..3 -> native executable / PIC object
                   -> wasm32 object -> LLD -> standalone .wasm
   -> (--backend fast, -O0 only) backend -> Cranelift -> native object -> Clang links it
                                          -> backend::wasm -> wasm32 object -> LLD -> standalone .wasm
```

| ファイル | 責務 |
|---|---|
| `src/diagnostic.rs` | ソース ID とファイル内位置、診断の順序・重複除去・表示／収集上限、human／JSON lines の出力制御。診断は `Diagnostic(Box<DiagnosticData>)` で 1 ポインター幅（再帰する検査の `Result<_, Diagnostic>` の一時値で stack を増やさないため。フィールドは `Deref` で読み書きし、move は `into_data`）。主の位置のほかに、関連する位置（`related`。`RelatedRole` の閉じた語彙）・`help`・修正案（`fixes`。`Fix` と `Edit`、`Applicability`）・`detail`（`expected`・`found`・`candidates`・`scope`）を持ち、ビルダーが上限（16・8・1 修正案あたり 64 の edit・10・32）を適用する（edit が 0 件か上限超えの修正案は丸ごと捨てる）。`render_with`・`json_with` は別のソースの位置を `SourceLookup` で引き、引けない `related` と、edit を 1 つでも引けない修正案は出さない。追加の情報が無い診断の出力は従来と同じバイト列。`apply_edits` は修正案の edit を重なりと文字境界を検査して適用する（LSP の検証が使う）。`DiagnosticSet.counts` は表示の上限で切る前の重要度・コードごとの件数 |
| `src/diagnostic_kinds.rs` | 診断の kind の登録簿（G23）。`diagnostic_kinds!` マクロが `since` の群ごとに（コード, id）の昇順で並べた `(Variant, "id", "CODE")` から `DiagnosticKind`・`ALL`・`id`・`code`・`since`・`from_id` を作り、`severity`（コードの先頭が `W` なら警告）・`kinds_with_code`・`codes` を足す。診断は `Diagnostic::of(kind, message, span)` でだけ作り（`code`・重要度・`kind` は登録簿から）、作った診断の分類を変えるときは `recode(kind)`。どの kind も `src/` のどこかで発行されることを `tests/diagnostic_kinds.rs` が検査する。kind は消さず、改名せず、別のコードへ移さない |
| `src/explain.rs`、`src/explain/` | `tsuzuri explain` の説明ページ（英語）。`explain/<kind>.md`（`KIND_PAGES`）と `explain/codes/<CODE>.md`（`CODE_PAGES`。全コード）を `include_str!` で埋め込み、実行時にファイルを読まない。`resolve`（kind の id か大文字のコード）・`suggest`（Levenshtein 距離と前置きで最大 5 件）・`render_kind`・`render_code`・`render_catalog`（テキストと 1 行 JSON）。kind のページの `Bad` が当該 kind を出し `Good` が通ることを `tests/explain.rs` が検査する |
| `src/diff.rs` | 行の差分（G24）。Myers の貪欲な前向き版（`shortest_script`。変更行が `MAX_DIFF_CHANGES` = 2,000 を超えると `None`）、`diff -u` と同じ統一差分（`unified`）、行単位の編集（`line_edits`。LSP の `source.fixAll` が使う）。外部の `diff` を呼ばない |
| `src/fix.rs` | `tsuzuri fix`（G24）。`run` はラウンドごとに、診断（エラー → 警告）の修正案から閾値以上の最初の 1 つを選び、書いてよいファイル（利用者のソースで依存でないもの・`Scope::File`・範囲と文字境界・4,096 bytes・4 MiB・同じ修正案の中の重なり）で検査し、整列して閉区間で交わるものを次のラウンドへ回し、メモリ上で適用して解析し直す。エラーの総数、`(path, code)` ごとのエラー（省略がないとき）、エラーがないときの警告のどれかが増えたら全テキストを戻して `E2008`。`MAX_ROUNDS` = 5、1 ラウンド 20,000 件を超えると `E1017`。書き込みは `driver::replace_files`（`fmt` と同じ保護と巻き戻し。`replace_source_atomically` とは別）。`--edition` は A18 の `edition_migration` に渡し、その前に `canonical_round`（A22 の `W1007` の `certain` の修正案。`canonical_warnings` は今は空）を適用する |
| `src/guide.rs`、`guide/`、`src/guide_std.rs` | `tsuzuri guide`（G21）。`guide/*.md`（level 0 のカードと level 1 の 4 章）と `AGENTS.md.tmpl`・`llms.txt.tmpl` を `include_str!` で埋め込み、実行時にファイルを読まない。出力の前に `prepare`: CRLF を LF に、`<!--if edition=YYYY-->`〜`<!--endif-->` の他の edition のブロックを除き（`filter_edition`）、`<!-- generated:NAME -->` の対の間を生成器の出力に置き換える（`expand_generated`。`pitfalls` は `hints::TABLE` の `help`）。ひな形も描画する edition で同じ処理をする。`guide_std` は std のソース（`parser::parse_with_source` と `docgen::signature`）と `Builtin::ALL` の署名から std の索引を決定的に作る。上限（D-56: 6,000・20,000・3,500・1,500・6,000・45,000 字）と全例の検査は `tests/guide.rs` と `scripts/check-guide.mjs`（例を全 edition で `check`・`run`・`test`） |
| `src/hints.rs` | ヒントの表（`TABLE`。40 件）と、失敗した解析の診断への適用（`annotate`）。`analyze_inputs_with` が失敗の `DiagnosticSet` に 1 回だけかける（成功の経路は変えない）。kind・メッセージの断片・主スパンの先頭のトークンの前後（`lexer::lex_all` をソースごとに 1 回）で選び、字句の条件で合ったヒントは発行箇所の `help` を置き換え、条件のないヒントは空の `help` だけを埋める。修正案は主トークンの置き換えで `likely`／`maybe` だけ。利用者のソース（`ModuleOrigin::User`）だけに付ける。G21 のガイドが `TABLE` を読む |
| `src/syntax.rs` | トークン定義、構文木（AST）、言語版 `Edition`、説明のタグの行の読み取り（`Documentation::deprecation`・`complexity`・`since`・`see`。共通の規則は私的な `tag_notes`）、構文リソース上限の管理 |
| `src/lexer.rs` | UTF-8 を壊さない字句走査、コメント処理（ファイル先頭の `#!` 行も行コメントとして飛ばし、位置はファイル全体のまま。`shebang_length`）、数値リテラルの切り出し、edition ごとの予約語（`EDITION_KEYWORDS`、`reserved_in`。`lex*` は edition を必須の引数に取る） |
| `src/docgen.rs` | 宣言 AST からの公開 API ドキュメント（Markdown）生成、型・制約・region の描画、決定的なページソート |
| `scripts/gen-docs.mjs`、`scripts/check-links.mjs`、`scripts/check-cli-docs.mjs`、`benchmarks/render.mjs` | 文書の生成物（G35）。`gen-docs.mjs` は `PLACEMENTS` の `<!-- generated:NAME -->` ブロックの本体を、コンパイラの出力（`explain --catalog --json` の診断 kind の表、`doc std` の `@complexity` から作る標準コレクションの計算量の表）と `docs/benchmarks-data/*.json`（`render.mjs` が Markdown の表にする）から作り、`--check` でずれを報告する（`check-docs.mjs` も呼ぶ）。`check-links.mjs` は `docs/benchmarks.md` へのリンクと見出し、`check-cli-docs.mjs` は `--help` の旗・サブコマンド・環境変数、診断コード、版、std の公開関数が文書に載っているかを検査する。CI は `.github/workflows/docs.yml`。`tests/gen_docs.mjs`・`check_links.mjs`・`check_cli_docs.mjs`・`benchmark_render.mjs`、Rust の `tests/doc_tags.rs`・`tests/std_tags.rs` |
| `src/doctest.rs` | doc の例（G34）。`examples_in_source` が `///` の本文から ```` ```tsuzuri ```` のフェンスを読み（記法の誤りは `E2011`）、`collect` が `tsuzuri doc` と同じ宣言から通しの番号つきで集め（上限 10,000、`E1017`）、`run` が例ごとに `Project::for_doc_example`（パッケージの `Main.tz` を例で置き換える）か `Project::single_main`（`std`）で `analyze_all` し、`run=` は `driver::run_captured_quiet`（30 秒、標準エラーを捨てる）、`test` は例の `Main` のテストだけを走らせる。例は `parallel::ordered_for_each` で並べ、結果の順は入力の順。`render_doc` は `tsuzuri doc` とホバーのために隠し行を除き `Output:`・`Compile error:` を足す（印も隠し行もない本文は 1 byte も変えない）。`tests/doctest.rs` が fixture（`tests/fixtures/doctest/`）で CLI を、`tests/std_examples.rs` が std の公開関数の例の数（`FLOORS`）と記法を検査する |
| `src/api_diff.rs`・`src/semantic_api.rs` | `tsuzuri api-diff`（G19・A19）。既定は root の公開宣言の構文比較。`--effective` は型検査中の解決済み署名・実際の引数規約・純粋性の段階・region の対応を明示的に収集し、安定した公開名で比較。breaking と non-breaking を決定的な順で出力（テキストと JSON Lines） |
| `src/parser.rs` | Pratt パーサー、宣言と式、トップレベルのエントリーコード、再帰深度の制限 |
| `src/parse_control.rs` | インデントによるブロック構文、for／while／match、関数ガード、ラムダ式、パターンおよびアクティブパターン認識器名 |
| `src/check.rs` | 全モジュールのシグネチャ収集、名前解決、型付き IR の構築、メモリレイアウト計算、公開 ABI |
| `src/control.rs` / `recursion.rs` | 型付きループ、短絡評価を行うパターン手順と束縛、認識器呼び出し、参照グラフに基づく再帰検査 |
| `src/type_arena.rs` | 型述語用の private TypeId／hash-consed Vec arena。型を借用・投影し、visited を ID で管理。公開 AST・Type・宣言順・診断順は維持 |
| `src/intern.rs` | `check::Names` が解析ごとに所有する名前 interner。`Rc<str>` を共有し、初出順の `Symbol(u32)` で検索。番号に `Ord` を付けず、HashMap の反復で診断・IR の順を決めない。構文木の識別子表現は維持 |
| `src/exhaustiveness.rs` | 型付きの被覆パターン、usefulness アルゴリズムによる match の網羅性検査・到達不能節の検出、不足パターンの提示（check の子モジュール） |
| `src/computation.rs` | ソース種別の検査、`.tc` ビルダーと `@alias` の収集、型検査前の関数・継続への展開（check の子モジュール） |
| `src/polymorph.rs` | 型変数の単一化、型クラス・インスタンス、モジュール関数制約の解決、制約の伝播、単相化（check の子モジュール） |
| `src/closures.rs` | 匿名関数の検査、自由変数の捕捉解析、lambda lifting、公開 ABI 向けの完全適用ラッパー生成 |
| `src/contracts.rs` | `check::contracts`（A21）: `ContractMode`・`ContractShape`、契約の行の型付き IR への織り込み（`Checker::contracted_body`。`$requires`・`$ensures` の呼び出しと `@old` の束縛を `Block` に並べる）、`off` の剥がし（`strip`。`polymorph::specialize` の直前）、条件の純粋性（`effects::impurities` による `E1033`。`Options::pure_values` で `pure` 関数型の値の呼び出しは効果なし、追えない関数値を呼ぶ関数（`Unknown::ValueCall`）は拒否）、条件の書き込み（`validate_writes`。呼び出しの記憶域を根とする場所、それを指す参照を持つ条件の局所（束縛と代入。参照外しを通す代入は、場所が読む局所とそれらが持ちうる参照先の局所）、条件分岐・`match`・ブロックの結果への代入・`ref mut`・可変の引数への受け渡し（呼び出しとパイプライン）と、`pure` でない関数の `ref mut` の欄を持つレコードの引数への受け渡しを `E1033`）、特殊化の前のジェネリックな関数の同じ検査（`validate_templates`。`Options::templates` で `GenericFunction` をそのテンプレートへの辺、ラムダの本体をその場の式として走査し、class のメソッドや追えない値はインスタンスの検査に任せる）、`--contracts test` の性質テストの生成（`generate_tests`。関数と同じ引数を持つ非公開の `$contract_<名前>` と、生成した引数を `$arg_<番号>` で束縛してそれを呼ぶ AST の `test`。引数は関数の呼び出しと同じ規約で渡る。必要な `Gen` は `Project::prepare_contracts` が解析の前に源へ加える）。契約の行の置き場は構文解析器が決め（`contract_site` が入れ子のラムダの本体へ進み、`site_lambdas` がその数を数える）、`Parser::define` の正規化で引数にならなかったラムダ（関数が返すラムダ）の中の行は `E1033`。`semantic` は `@ensures` の結果の名前を行ごとの定義にし、織り込みの `$contract.result` の読みをその行の名前の参照として索引する |
| `src/canonical.rs`・`src/parse_canonical.rs` | A22 の正準形。構文解析が非正準な表記（`for` の素の `..`、短い接尾辞、記号の借用・参照外し・参照型、旧ラムダ）を `Program.noncanonical`（`Noncanonical`: kind・主の位置・綴り・正準形・確度・`TextEdit`）に記録し（`record_*`。借用は被演算子が場所なら `certain`、それ以外は `likely`、型検査が参照と解いた `&r` は修正なし）、edition 2027（`Edition::rejects_noncanonical`）は構文解析の段で `E0004` を出す。`CheckedModule.noncanonical` は根のパッケージの User のソースだけで、`canonical::warnings` が `W1007` と修正案にし、`fix::canonical_warnings`（`fix --edition` の canonical round）が使う。範囲の `..=`・`..<`（`RangeOp`）は `ForRange.exclusive` と `Slice` の `b + 1` に下がり、`llvm_control.rs` の `range_loop` は半開の前置き（`half_open_last`）のあと閉区間のループ（`closed_range_loop`。切り出しで IR は不変）を回す |
| `src/capabilities.rs` | 信頼境界と権限（E15）。`Capability`（9 個）・`CapabilitySet`、マニフェストのポリシー（`Policy::build`。ルートの `[capabilities]` と依存の `capabilities` から、入辺の積で実効の権限を求める。`blocker` は阻んだ制限）、型検査後の型付き IR の走査（`analyze`。家族の根でソースとパッケージを決め、私的な組み込み関数の表 `primitive` と `Gpu` の明示から std の関数の権限を不動点で求め、std でない関数の参照を地点にする。`$builtin` のラッパーは共有されるので参照した側のファイルで分類する。コンパイラが足す入口の使用は合成の地点で、結果を表示するトップレベルのコードは `stdio`、引数を受け取る `def main` は `env`、`extern` の `main` は宣言を地点にした `ffi` と extern の使用）、`E1036`（`denied`）・`W1010`（`untrusted_externs`）、`tsuzuri audit` の `Report`（text と `tz-audit/1` の JSON Lines。extern の一覧は宣言したパッケージの `PackageId` で分け、名前では分けない。`effects` の数は書かれた宣言ごとに 1 回数え、ジェネリック関数は特殊化が具体化と別に残す `CheckedModule::generic_declarations` から数えるので、具体化の数によらず、呼ばれないものも 1 つ）。`Project::check_capabilities` はポリシーが有効なときだけ解析する。生成コードは変えない |
| `src/prove.rs`・`src/prove_smt.rs` | `tsuzuri prove`・`--contracts prove`（A21 Phase 5）: 織り込んだ関数を `QF_BV` の SMT-LIB に翻訳し（トラップは値でなく義務として集める）、問いごとに外部ソルバーを起動し、反例を復元して報告する。`binary_term` は差分テストの被験 |
| `src/effects.rs` | `check::effects`（A20）: 特殊化と `closures::lower` の後の型付き IR から関数ごとの効果の要約（D-53 の 14 個のビット集合と「たどれない呼び出し」）を推論する。関数の参照を辺とする呼び出しグラフの強連結成分は同じ要約を持ち（Kosaraju、反復）、証拠は直接の源から幅優先で伝える。組み込み関数は wildcard のない `builtin_facts` の表（A19 の `pure_rules::builtin_impure` と facts の `kg::effects::builtin_effect` と一致を単体テストで固定）。`alloc` はエミッターの `llvm::frame_bytes`・`llvm::staying_locals` を再利用する位置規則、`trap` は `ranges::analyze` と `enter_else_condition`、`copy` は `copies::sites`。関数型の引数の呼び出しは `calls_params` に移し、属性つきの関数の呼び出し側で実引数を検査する。属性（`Program.effect_attributes` → `FunctionOrigin.asserts`。特殊化のインスタンスは保ち、生成関数は持たない）がなければ何も計算しない。`validate` が `E1031`・`E1032`・`W1008` を作る。生成 IR は変えない |
| `src/closures_rec.rs` | ローカルな `let rec`（`and` のグループを含む）のリフト。グループの捕捉の和、値渡しと借用の分類、本体ごとのフレーム（どの `let rec` 関数を直接呼べるか）、予約した id、直接呼び出しへの書き換え（closures の子モジュール） |
| `src/numeric.rs` | プリミティブ型名、整数・浮動小数点接尾辞、binary／decimal リテラルの丸めとエンコーディング |
| `src/constants.rs` | 定数の依存順評価、型ごとの演算とリソース上限、既存リテラルへの展開、定数・一時値の借用引数管理（`temporary_borrows`） |
| `src/const_rules.rs` / `src/const_machine.rs` / `src/const_static.rs` | `const def` のコンパイル時評価と静的データ（`constants` の子モジュール）。`const_rules` は署名の値型の検査と、本体の式（`TypedExprKind`）ごとの受理・拒否の網羅的な振り分け、`const_machine` は明示的なスタックを持つ評価器（`Machine`）で、呼び出しも反復も Rust の再帰を使いません。`const_static` は平たいデータの定数を判定し、読み出しと借用を `Static` の借用へ書き換え、静的データの大きさを数えます |
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
| `std/Net.tz` / `src/runtime/net.c` | ネットワーク API（E09）。opt-in の std ソース（アドレスの解析と表示は純粋な Tsuzuri。`Async` を使う）、`Net.__*` 組み込み関数 12 個（`src/llvm_io.rs` の `net_builtin`）、POSIX と Winsock の socket ランタイムと、非同期の待ちを見る 1 本の監視スレッド。IR が `@tsuzuri_net_` を参照したときだけ native の単一翻訳単位へ `os.c` の直後に連結 |
| `src/runtime/bindings-net.mjs` / `bindings-net-host.mjs` | `--wasm-feature net` のグルー（E09 Phase 3）。`net.c` と同じ約束を Node.js の `net`・`dgram`・`dns` で実装する `tsuzuri_net` の import 14 個と、Node.js のモジュールの読み込み。`src/bindings.rs` の `javascript_with_net` が `bindings-core.mjs` へ差し込む（ソケットに到達するモジュール（`llvm::reaches_net`）だけ） |
| `std/HashMap.tz` / `std/HashSet.tz` | ハッシュコンテナ。コンパイラ本体に専用の型・builtin・ランタイムを追加しない、std ソースのみによる実装 |
| `std/Regex.tz` / `std/Unicode.tz` / `src/runtime/unicode.ll` / `scripts/generate-unicode.mjs` | 線形時間の正規表現（std ソースの Pike VM）と Unicode の表。表は UCD 17.0.0 から生成したランタイム定数で、std 専用の組み込み `Unicode.__table_length`／`__table_entry` が読む |
| `std/Json.tz` | JSON（RFC 8259）の解析・出力、`Json.Value`、組み込みクラス `Encode`／`Decode` の std インスタンスと導出用の補助関数。コンパイラは 2 クラスの登録（`Classes::collect`、シグネチャは std の `Json.Value`／`Json.Error`／`Result` から引き、std に無ければ使用時に `E1004`）と導出（`src/derive.rs`）だけを持ち、専用の `Type`・builtin・ランタイムはない |
| `std/Cbor.tz` | `Json.Value` の CBOR（RFC 8949）。決定的な符号化と、JSON のデータモデルに限った厳密な復号。浮動小数点のビット列は 2 のべきの厳密な拡大・縮小で求め、専用の builtin を使わない |
| `std/Format.tz` / `src/runtime/format.ll` | 文字列補間の書式指定。`Format.parse`／`Format.pad` と、パディング処理のランタイム補助（文字列結合は `src/llvm_display.rs`） |
| `src/simd.rs` / `src/llvm_simd.rs` | 128-bit・256-bit の vector/mask 型、lane 型族、境界検査、LLVM vector への lowering、256-bit の load／store の `align 16` |
| `src/llvm_cpu.rs` | `@cpu` 関数の level ごとの版、版を選ぶ stub、256-bit ベクトルを渡す呼び出し先の版（F08 Phase 3）。x86-64 の版の `target-features` には、f16 の経路が要る `+f16c`・`+avx512fp16` も足す（D10） |
| `src/llvm_control.rs` | 直接反復・switch・定数テーブル生成、パターンマッチ手順の分岐展開と全経路での解放処理 |
| `src/llvm_bulk.rs` | 配列連結・リストの一括走査・安定なマージソート（merge sort）の型付き builtin lowering |
| `src/llvm_list.rs` | リストの表現（PM04）。chunk の構造、一括生成・`List.cons`・`List.tail`・走査・索引・tail の view・複製・解放・スタックのリテラルの helper。リストの配置を知るのはこの module だけです |
| `src/llvm_compare.rs` | 配列・リスト・タプルの借用構造比較、短絡評価と段階的な比較メソッド適用 |
| `src/derive.rs` / `src/llvm_hash.rs` / `src/llvm_display.rs` | 導出（derive）instance の AST 合成、canonical Hash、引用符・コレクション表示、文字列補間（`Interpolated`）の一括結合 |
| `src/llvm_math.rs` / `src/runtime/math.c` / `src/runtime/musl/` | Float 基本数学演算と型別 Elementary 関数、固定バージョンの移植可能な数学関数実装。f16 を LLVM の `half` 命令で計算する経路の生成（`half_binary`・`half_sqrt`・`half_cast`、D10） |
| `src/recursive.rs` / `src/llvm_recursive.rs` / `src/runtime/recursive.ll` | 具体型ごとの再帰成分、所有ノード、追加確保を伴わないスタックレス解放と反復複製 |
| `src/llvm_layout.rs` | 値のレイアウト（PM01）。union のタグ幅と `General` の領域・niche、record のフィールドの格納順（`record_slots`・`field_slot`・`physical_fields`）、サイズの上下限モデル（`layout_bound`／`storage_layout`）、タグの読み書き（`tag_value`・`load_tag`・`construct_value`）、`target_scope` と `union_layout`・`record_slots` の memo。レイアウトに依存する処理はここを通す |
| `src/llvm_frame.rs` | `new` を伴わないリテラル・小さな文字列連結・局所の捕捉環境のスタック配置、実行時アドレス判定、スコープ外移動時のヒープ移送、フレームを考慮した解放 |
| `src/llvm_escape.rs` | immutable な局所の使用位置による滞在判定と、関数・lift 済み閉包の参照グラフの循環判定。PM06 の領域選択だけに使い、正しさは frame の移送・解放で保証 |
| `src/llvm_facts.rs` | 内部参照の属性、上限付き SSA provenance、scoped alias・限定した scalar TBAA、値と memory effects の証明。`llvm.rs` の portable layout 下界と共有 metadata 番号を使い、不変性は `Type::is_frozen` で判定。opaque std・共有可変状態・隠れた環境を不変と推測しない |
| `src/llvm_return_slot.rs` | PR03 の戻り値 slot。再帰しない、tag と payload を持つ union（`UnionLayout::is_aggregate()`、`Common` と `General`）を返す関数の複数の `ret` を、入口の alloca への store と 1 つの `tz.return` ブロックの load・`ret` に書き換える post-pass（`facts::apply` の後）。tag だけの enum と、payload そのものの niche union（`Maybe<string>`・ネイティブの `Maybe<ref T>`）は対象外で、平文の `ret` を保つ。slot は 1 KiB 以下。呼び出しの結果を返しうる exit（呼び出し・パイプ・引数でない局所、それらの欄）が 1 つでもあれば全 exit を平文の `ret` のまま残し、LLVM の末尾呼び出しを守る |
| `src/last_use.rs` | 具体化 IR の後方生存解析 `LastUses`。所有権検査の State／loan とは独立にループ・match を固定点で扱い、訪問 100,000 回・深さ 128 の予算を超えると空の結果へ fallback。生成と `copies::sites` が結果を共有。全体 Local の最後の使用と、所有する match 束縛の非重複射影を判定し、一般の Field 根には単一使用の証明だけを使う |
| `src/llvm_sync.rs` / `std/Atomic.tz` / `std/Mutex.tz` / `std/Channel.tz` / `src/runtime/sync.ll` / `src/runtime/sync-wasm.ll` / `src/runtime/channel-wasm.ll` | F10。`Atomic` の `seq_cst` の `atomicrmw`・`cmpxchg`・`load atomic`・`store atomic`、`Mutex.create`／`with_lock`／`into_inner` の lowering（ロックの取得・解放と、入れ子・並列開始の拒否）、`Channel` の lowering（ブロックの確保と初期化、要素の drop、ランタイム関数の呼び出し）。`sync.ll` はロック中に並列開始を拒むラッパー、`sync-wasm.ll` は既定の wasm32 のロックのフラグ、`channel-wasm.ll` は既定の wasm32 の `Channel` のランタイム |
| `src/call_specialization.rs` | エスケープしない関数引数の不動点解析、既知の継続・読み取り専用捕捉の判定、LLVM worker 関数の特殊化予算管理。予算（1,024 個）を使い切った program だけ、関数と参照先の関数を深さ優先の順に出力し、各関数の worker をその直後に出力する 2 回目の出力（`Drain::DepthFirst`）で、連鎖を完成させる |
| `src/ranges.rs` | 型付き IR 上での配列添字の範囲証明（規則 R1–R5）と関数単位の `RangeFacts`。証明された添字のみ境界検査の分岐を省略。PR02 は同じ `RangeFacts` に整数の区間（`values`・`undo`・`steps`）を持たせる。`enter_else_condition`（A20）は `else` 側で `not (a \|\| b)` の比較を否定した守りを積む。効果の要約だけが呼び、エミッターは呼ばない（生成 IR は変わらない） |
| `src/interval.rs` | PR02 の整数の区間の純粋関数。端点は `i128` で、64-bit 以下の整数型だけを扱う。`+ - *` の `nsw`／`nuw` と結果の区間、`as`・`&`・`%`・`/`・シフト・`~` の区間を返す。オペランドを符号付きと符号なしの両方で読み（読みが切り替わる点をまたぐ区間は全域の hull で読む）、返す集合は常にそのとき取りうる値をすべて含む |
| `src/ranges_intervals.rs` | `RangeFacts` の整数の区間（リテラル、型、キャスト、マスク、長さ、`if`、`let`、`for` のカウンター、`if` の条件による絞り込み）と `integer_flags`。深さ 32 と 1 関数あたり 2^20 ステップを超えた分は型の全域 |
| `src/llvm_ranges.rs` | `TSUZURI_RANGE_FLAGS`（`RangeFlags`: `off`／`proven`／`audit`）の読み取りと旗の書き分け。audit は旗を書かず、主張した向きごとの `with.overflow` の検査と `TrapKind::Overflow` の trap を書く |
| `src/runtime/integer.ll` | 整数（`i8`〜`i128`、`i8u`〜`i128u`）の 10 進表示、書式指定付きの表示、`Parse`（PM08）。手書きの IR で、`numeric.ll` に依存しません。`@tz.integer.format`（64 bit 以下を `i64` へ拡張して受ける）と `@tz.integer.format128`、書式指定の `@tz.integer.spec`／`@tz.integer.spec128`（`+`・`x`・`X`・`o`・`b`。`flags` は `tz_soft_format_spec` と同じ）、UTF-16 のコード単位を直接読む `@tz.integer.parse`／`@tz.integer.parse128`、それらが使う内部の `digits`（10 進の桁を 2 桁ずつ書きます）・`divide128`・`magnitude128`・`radix`・`sign`・`digit`・`scan` を持ちます。テキストを書く関数はバッファの末尾から前へ書き、呼び出し側は 10 進に 40 byte（最長の `-170141183460469231731687303715884105728`）、書式指定に 160 byte を渡します。`i128` の乗算は 128 bit の入口だけが使うので、`i128` を扱わないプログラムに `__multi3` は要りません。`i128` の除算は、どこにも書きません（10 進の桁は 10^9 で割る 32 bit 桁の筆算 `divide128` で、`Parse` の上限もそれで割ります）。LLVM は `i128` の除算を `__udivti3` の系列の呼び出しにしますが、Windows の x86-64 ではその呼び出しがオペランドをポインターで渡して XMM0 で返し、Windows のビルドに連結する `wasm.ll` の定義（`i128` を 2 つのレジスタで受ける）がその規約を守らないので、桁が壊れます（PR #27 の Windows の CI で判明）。`tests/prebuilt_runtime.rs` が `integer.ll` に `udiv i128` などがないこと、`tests/integer_format.mjs` が Windows（x86-64・arm64）・Linux・macOS 向けの assembly に `__udivti3` の系列の呼び出しがないことを確かめます。`to_string`・`Display`・補間（書式指定を含む）・トップレベルの式の結果・整数の `Parse.parse` が使います。`tz.integer.` の記号が IR に現れたときだけモジュールへ連結する `define internal` で、メタデータも `declare` も持ちません。`tests/integer_format.mjs` が Python の独立した参照、全 8／16 bit 値、`numeric.ll` の `tz_soft_*`（同じ入力）と照合します |
| `src/runtime/numeric.c` / `numeric.ll` / `numeric_tables.h` | 多倍長整数を用いた f16／f128／decimal 演算、比較、広幅・異形式間の変換、書式指定付き数値表示（`tz_soft_format_spec`）。f32／f64 の最短表示は整数 Schubfach、通常の十進解析は符号を除き最大 25 ASCII 文字・19 有効桁の Eisel–Lemire と正確な fallback。長い入力を高速経路で再走査しない。生成する不変表は 20,288 bytes。f16 を `half` 命令で計算するビルドでも、結果のビット（NaN を含む）はこのルーチンと同一（D10） |
| `src/runtime/string.ll` / `utf8string.ll` / `string_scalar.ll` / `string_v128.ll` / `string_cpu.ll` / `string_cpu_utf16.ll` / `heap-*.ll` | UTF 共通処理、完全な scalar 参照、汎用 vector IR、native CPU wrapper。`generate_utf.py` の生成物とは別に `string_cpu_utf16.ll` は手書きの x86 専用 wrapper。変換は全入力の検証・計数後に一度だけ確保。native CPU 経路は F08 の能力・OS 状態検査と fallback、WASM は既定 scalar・simd128 指定時だけ vector IR。Windows の CPU runtime は portable baseline。UTF-8 復号の 16 bytes 未満は inline scalar、native の 16–31 bytes は portable LLVM、32 bytes 以上は CPU 経路。UTF-16 符号化は non-x86 では portable LLVM、native x86 の 32 単位以上は CPU count と検証済み入力を扱う LLVM writer。portable 符号化の 16 単位未満／初窓に surrogate がある入力は元の scalar 計数・Unicode write と 8 単位幅の ASCII copy を使う。長い bulk 経路は private noinline helper へ分離。計数は 1／2／3-byte 幅の混在 BMP も扱うが、writer は均一幅の prefix だけを一括処理する。順序比較の UTF-16 16 単位未満／UTF-8 32 bytes 未満は元の 64-bit/scalar 経路。SIMD 経路の UTF-16 の検証・修復は surrogate-free prefix の後、残りを scalar へ一度だけ渡す。変換・計数・UTF-8 検証 kernel も最初の failed bulk probe 後は scalar remainder へ一度だけ渡し、後続の ASCII 区間も再 probe しない。文字列表現・コード単位の符号なし比較・置換・trap・ABI・import は維持（`TSUZURI_STRINGS=compact` の内部表現は次の行） |
| `src/llvm_strings.rs` / `src/runtime/string_latin1.ll` / `string_latin1_v128.ll` / `string_compact.ll` / `display_compact.ll` / `character_compact.ll` / `format_compact.ll` / `integer_compact.ll` | PM03 のコンパクト文字列（`TSUZURI_STRINGS`。`StringForm`: `utf16`／`compact`）。`src/llvm_strings.rs` が環境変数の読み取り（空・未設定は `utf16`、ほかは `E2000`）、runtime の層の合成（`layered`: 既存の定義のうち、層が取り直すものの `define` 行だけを `@名前.utf16` へ改名して残し、層が元の名前を定義し直す。`utf16` では何もしないので IR は PM03 より前と byte 単位で同じ）、Latin-1 を作る・読む LLVM の補助（`marks_length`・`string_word`・`string_count`・`string_length`・`string_is_latin1`・`string_unit`・`string_unit_pointer`・`string_literal`）、`compact` の `String.slice`／`String.join` の本体（`compact_body`。確保 1 回）を持つ。`string_latin1.ll` は Latin-1 の kernel の scalar 参照（確保・複製・広げる・狭める・等しさ・順序・`to_ascii`・UTF-8 の走査 `utf8_scan` と変換 `utf8_to_latin1`・符号化）、`string_latin1_v128.ll` は同じ関数の 128-bit vector 版で、`generate_utf.py` が書く生成物（手で編集しない。`tests/utf_simd.mjs` が再生成した結果との一致を確かめる）。`string_compact.ll` は、形式（長さの語の bit 62）を見て Latin-1 の kernel か `.utf16` の元の関数へ振り分ける取り直し（`new`・`concat`・`append`・`concat_into`・`equal`・`compare`・`from_utf8`・`to_well_formed` など）、host の境界の `to_utf16`／`borrow_utf16`、`from_units`、`slice`。`display_compact.ll`・`character_compact.ll`・`format_compact.ll`・`integer_compact.ll` は、表示・`char`・書式指定・整数の `Parse` が Latin-1 を直接作る・読む層。`compact` のときだけ連結し、vector 版は native と `--wasm-feature simd128` の WebAssembly で選ぶ |
| `src/runtime/closure.ll` | 関数値の環境の複製および解放。静的記述子の clone／drop 欄を読んで呼び（drop が null の即値の環境は何もしない）、環境ごとの固有処理は LLVM emitter が生成 |
| `src/runtime/cpu.c` | 整数 8 型の配列の和・最小・最大、および UTF の level ごとの kernel。CPUID/OSXSAVE/XCR0 と `getauxval` による機能検出、アトミックな level キャッシュ、`@cpu` 関数の版を選ぶ `tsuzuri_cpu_pick`。UTF の public runtime から全操作を C へ渡すわけではなく、上記の長さ・ISA による選択を使う |
| `src/runtime/heap-*.ll` の `tz.realloc` | ネイティブの realloc、および WASM における終端でのその場の拡張、隣接空き領域の吸収と、確保・`memory.copy`・解放へのフォールバック |
| `src/runtime/task.c` / `task-wasm.ll` / `task-wasm-threads.c` | ネイティブ常駐スレッドプール（と F10 の `Mutex` のロック、`Channel` の待ち・起こし・デッドロックの判定）、WASM デフォルトの逐次実行、opt-in の共有メモリ Worker プール（と、その `Mutex` と `Channel`） |
| `src/runtime/wasm.ll` | 128-bit 乗除算・剰余・ビットシフトの freestanding 補助関数群 |
| `src/stdlib.rs` / `std/` | 埋め込み標準ライブラリのソースコード、予約 std モジュール名、std の仮想パス解決 |
| `src/driver.rs` | ソースファイルの列挙、`Main.tz` の選択、LLVM／LLD の起動、ステージング、出力保護。ツールは `TSUZURI_*` → 配布物（実行ファイルの2階層上に `manifest.json`）の `bin/` → `PATH` の優先順で解決（`resolve_tool`。キャッシュキーにも同一の解決ロジックを使用）。native の target ごとの Clang 引数（`compile_args`・`sdk_args`・`link_args`）、`TSUZURI_SYSROOT`、foreign target の実行器（`foreign_runner`）。f16 の経路は純関数 `f16_path`（WASM、ターゲットの OS・アーキテクチャ、`--cpu`、`CpuFeatures`）が選び、`--cpu native` の x86-64 では clang へ `-mf16c`・`-mavx512fp16` も渡す（D10） |
| `src/profile.rs` | プロファイル誘導最適化（PR07。driver の子モジュール）。`Profile`（なし・`Generate(DIR)`・`Use(PATH)`）の検査（`validate`: native の `exe`・`object`、wasm32 の `wasm` だけ。Windows・別の機械の target・`--freestanding`・wasm64・threads は `E2000`）、同梱の launcher の `E2002`、`--profile-use` の準備（ディレクトリは `llvm-profdata merge` を名前順に実行して 1 つにし（path は引数でなく `--input-files` の一覧のファイルで渡す）、ファイルは複製して magic を確かめ、中身の SHA-256 をキャッシュの鍵に渡す。raw 1 つは `E2000`）、`llvm-profdata` の探索（`TSUZURI_LLVM_PROFDATA`、Clang の隣、`-print-resource-dir` の 3 階層上。PATH は見ない）、合わない関数の警告の 1 行への要約（`W2001`）、計装のリンクの `ld` が x86_64 の macOS で出す整列の警告 3 行（`__llvm_prf_*`）の除去、`run --profile-generate` が書かなかったときの警告、wasm32 の書き出し役（`src/runtime/profile-wasm.mjs`）と IR・link の追加。`src/driver.rs` は呼び出しだけで、LLVM IR は変えない |
| `src/backend/` | `--backend fast`（PB05。cargo feature `fast-backend`）。`-O0` の LLVM IR のテキストを Cranelift で native の object に、または直接 wasm32 の object にする翻訳器。読み取り（`lex`・`module`・`func`・`parse`・`types`）、lowering（`lower`・`ops`・`mem`・`call`・`intrinsics`・`abi`・`data`・`target`）、決定的な並列 compile（`mod`）、`-g` の DWARF（`debug`。gimli）、E2002 の表（`error`）。`wasm/` は wasm32 用の lowering と、`wasm-ld` が読む再配置可能な wasm の object の書き出し（`bin`・`object`・`abi`・`lower`・`ops`・`mem`・`intrinsics`）。LLVM の C API には結合せず、Cranelift は Rust の依存として入る |
| `src/bin/tz-fast-cc.rs` | `.ll` を 1 つ翻訳するだけの補助 binary（feature `fast-backend` のときだけビルド）。`tests/fast_backend.mjs` が `TSUZURI_CLANG` の代役に使う |
| `src/target.rs` | 対応する 5 環境の 9 個の native の `Triple` の閉じた表（Clang の target、OS、CPU。Linux の glibc/musl と Windows の MSVC/MinGW を区別）、ビルド先の性質を答える `Platform`、`--target` の値の解析（`parse_target`）、foreign target の実行器 `TSUZURI_RUNNER`（`ForeignRunner`）。Windows x64/Arm64・Linux x64/Arm64・macOS Arm64 以外は受け付けない |
| `src/main.rs` | CLI オプションの解析と診断・警告の表示、`toolchain info`、`fetch`、`publish`、`api-diff`、`bindgen`、`repl` のオプション、`script` の引数の取り分け（ファイルより前を `run` のオプションとして解析し、後ろをプログラムへ渡す）。プロジェクトを読むコマンド（`parse_arguments` 以降）は `cli(raw, program_arguments, out, err)` で、診断と警告を `err` に書き、終了コードを数値で返す。`watch` と `serve`（`CommandServer`）はこの `cli` を、`ResidentState` の `Scope` の中で繰り返し呼ぶ。`check` と `build` の `cli` は、プロジェクトを読む前に `server::request` を試す |
| `src/watch.rs` | `tsuzuri watch` の変更検出（PB06）。最後の実行が読んだファイル（ユーザーのソースとマニフェスト）の本文と `FileStamp` を覚え、100 ms ごとに stat を比べ、1 s ごとにディレクトリを探し直す。新しい crate は使わない |
| `src/resident.rs` | 実行をまたいで持つ状態（PB06）。`ResidentState`（読んだソースの本文と内容ハッシュを `FileStamp` つきで覚え、起動したコンパイラの識別を固定する）と、それをコマンドを実行するスレッドに入れる `Scope`。`read_source_text`（`driver.rs`）と `file_digest`（`cache.rs`）が問い合わせる。`Scope` がないときは何も覚えず、1 回だけ走るコマンドは変わらない |
| `src/server.rs` | 常駐ビルドサーバー `tsuzuri serve`（PB06、Unix だけ）の protocol・私用ディレクトリ・ソケット名・環境の比較・主ループ（accept スレッドと、要求を 1 つずつ実行するスレッドと、ログを書くスレッド）と、`check`・`build` が要求を出すクライアント `request`、`serve --status／--stop` の `control`。`cli` は知らず、実行は `Handler`（`main.rs` の `CommandServer`）に任せる |
| `src/package.rs` / `src/fetch.rs` | マニフェストの限定 TOML（`[package]` の `edition` を含む）、版（`Version`）、`Tsuzuri.lock` と registry index の厳密な JSON と正規形、内容ハッシュ、`tsuzuri fetch` による git・registry 依存の取得・最小版選択・検査・ストアへの確定、`tsuzuri publish`（`git` を起動する唯一の経路） |
| `src/bindgen.rs` / `src/bindgen_driver.rs` | `tsuzuri bindgen`（E11）。前者は Clang の JSON AST から宣言の所属ファイルを追跡し、型の表記を typedef 展開して固定の表と完全一致で照合し、名前の規則と `W2002` の理由を適用して決定的なテキストを作る純関数だけを持つ。後者（driver の子モジュール）はヘッダーの読み込みと SHA-256、Clang の起動（stdout は 256 MiB まで、stderr は別スレッドで読む）、LP64 の確認、出力保護とステージングを伴う書き込みを行う |
| `src/copies.rs` | 単一使用と `LastUses` の証明を生成と共有した暗黙の複製箇所の列挙（`copies::sites`）、`W1006` とインレイヒントの基となる配列・リストの複製検出（`costly_sites`） |
| `src/parallel_report.rs` | `check --report parallel`（F15）の報告。特殊化と `closures::lower` の後の利用者の関数（ラムダを含む）を明示のスタックで走査し、`Array.map`・`Array.mapr`・`Array.fold_monoid`・整数の `Array.sum` の呼び出し（`Call` の呼び先が std の特殊化）を `Parallel` に置き換える提案と、`TypedExprKind::Parallel` と std の `Parallel.sum`・`Parallel.fold`（本体の `Parallel.reduce` の reducer）の呼び出し地点のコールバックの純粋性（`parallel_safe`）を作る。純粋性は D-53 の窓口（`pure_rules::reachable_effect`。`for_each_chunk` の本体は排他チャンクの引数を除いて検査する `reachable_effect_of_chunk_body`、組み込み関数は `builtin_impure`）に聞き、外から入る関数値（引数・結果・捕捉の型が関数型に届く、局所の関数値）と、関数値を返す根（段階的に適用され、A19 の到達検査は返した閉包を辿らない）は純粋としない。`Parallel.sum`・`Parallel.fold` は reducer に加えて std の本体全体（`Monoid.empty ()` の評価を含む）を `reachable_effect` に聞く。edition 2027 の読み取りコールバックのアダプタ（`$convention.N` の生成された局所を束縛し、自分の引数だけを渡して呼ぶ閉包）だけを剥がす。2027 の `Array.map`（呼び先は `Array.mapr`）でアダプタのない読み取りコールバックが、ヒープを持つ要素（`needs_drop`）を読むときは、置き換え先を `Parallel.map_ref` にする（`Parallel.map` は要素を複製する）。ただしコールバックが、引数が共有参照の名前のある関数（`Function` で、型の唯一の引数が `Reference(_, false)`）のときだけで、ラムダは読み取りの規約で値を見ているので、`ref` の引数では本体の型が変わりうるとして提案しない。特殊化ごとに置き換え先が違う位置は提案しない。`Sources::user_module("Parallel")`（`Project` は std でないソースのファイル名の幹で判定）が真なら、置き換え先を `std::Parallel.*` と書く。提案は名前がソースにそのまま書かれた位置だけで、`maybe` の `fixes` で自動では適用しない。エントリは（パス・開始・終了・種類・名前）の `BTreeMap` で特殊化の複製を 1 件にまとめ（提案は全部の複製が条件を満たすときだけ、地点は全部が純粋なときだけ `parallel_safe`）、1,000 件で切る。`build`・`entries`・`render_text`・`render_json`。KG の facts の `parallel` も `entries` から作る |
| `src/kg/` | 知識グラフ（KG。G26・D-60）。行の型と JSON Lines の書き手（`model.rs`・`json.rs`）、ID と型の ID テキスト（`ids.rs`）、グラフのアルゴリズム（`algo.rs`: CSR の隣接、層ごとの幅優先、Tarjan の強連結成分、最短路、トポロジカル順。どれも明示のスタック・キュー）、単相化後の実体の視点の事実（`facts.rs` が最終の関数を宣言へ帰属させ、`effects.rs`・`sites.rs`・`layout.rs` が効果・トラップと確保の箇所・レイアウトを作り、facts の `parallel` は `parallel_report::entries` を関数の宣言ごとに分ける。F15） |
| `src/kg_capture.rs` | `check` の子モジュール。所有権の推論の後・特殊化の前に、単相化前の宣言の視点（ノード、`contains`・`calls`・`uses_type`・`implements`・`instance_of`・`imports`・`tested_by` の辺、テスト、再帰・ループ・所有権の出来事）を、checker の状態を読むだけで集める |
| `src/kg/{settings,input,manifest,write,emit,build,open,store,brief,std_kg}.rs`、`src/main_kg.rs` | 知識グラフのファイル（G27・D-60）。`--kg`・`--no-kg`・`--kg-dir` と環境変数の決定（`settings`）、入力のハッシュとコンパイラの stamp（`input`）、`manifest.json` と鮮度（`manifest`）、マーカー・ファイル名・一時ディレクトリからの置換（`write`）、行のファイル（`emit`）、解析の前の計画と解析の後の書き出し（`build`）、照会のための読み込み（`open`・`store` の `KgData` と `KgIndex`）、brief カード（`brief`）、std の知識グラフ（`std_kg`）。`tsuzuri kg` のコマンドは `src/main_kg.rs` |
| `src/kg/{query,suggest,find,impact,git}.rs` | 知識グラフの照会と変更の影響（G28・D-60）。ID の解決と照会（`query`: `brief`・`show`・`callers`・`callees`・`uses`・`tests-for`・`search`・`path`）、未知の ID の候補（`suggest`: OSA の編集距離）、型での検索（`find`: 署名の型の解析と単一化）、2 つの知識グラフの差と届くテスト（`impact`）、`--git` の一時 worktree（`git`） |
| `src/find.rs`、`src/main_find.rs` | 型つき穴と未解決名の候補（G25）。`check` の子モジュール `find.rs` は、穴の `E1030`（`Checker::typed_hole`: 期待される型・`scope`・候補）、穴が探す宣言の索引（`find::build`。`def`・`const`・組み込み関数・field・case の型つきの形を、最初の穴で 1 回だけ `Names.search_index`（`OnceLock`）に作る。組み込み関数の型は使い捨ての `Checker` の `builtin` で得て、型族で結果が決まるものは除く）、候補の順位づけ（`hole_candidates`。候補ごとに新しい `Inference` で単一化し、制約が決まった型では `Classes::holds_class` を確かめる）、未解決名の候補（`suggest`: OSA の編集距離と `with_suggestions`）を持つ。`tsuzuri find` の前段は `src/main_find.rs` で、照合は `kg::find`（D-60）を std の知識グラフと `--project` のプロジェクトの知識グラフに対して呼ぶ |
| `src/diagram/` | 知識グラフの図（G30・D-60）。行の読み込み（`input`: `Input::from_kg`・`from_texts`）、ID の解決（`resolve`。`kg` の照会の段を使う）、エスケープ（`escape`）、フローチャートの 1 つのモデルと畳み込み（`graph`）、Mermaid と DOT の書き手（`mermaid`・`dot`）、7 種の図（`module`・`types`・`calls`・`ownership`・`concurrency`・`layout`・`transitions`）、`views/*.mmd`（`views`）、生成した形だけを受理する検査（`validate`） |
| `src/lsp.rs` / `src/semantic.rs` | stdio 経由の言語サーバー、Unicode 位置変換、単相化前の型・定義位置インデックス。定義・参照・ローカル変数の有効範囲・record 型の式をインデックス化し、型付き木で脱落するフィールド名・record 名・case 名は checker の `name_uses` から収集。リネームとクイックフィックスは編集後の再解析により診断と名前の結び付きの不変性を検証。診断の修正案のクイックフィックスは、適用後の再解析でその診断のコードの件数が減り、どのコードも増えないものだけを出す（`fix_removes`。件数は表示の上限の前の `DiagnosticSet.counts`。エラーの修正案は警告を数えない）。`publishDiagnostics` は利用者のファイルにある `related` を `relatedInformation`、`kind` と修正案を `data` にする。入力中の補完・シグネチャヘルプ・セマンティックトークン・複製のインレイヒントは、直前の成功インデックスを共通の接頭辞・接尾辞に基づいて写像して再利用 |
| `src/mcp/` | MCP サーバー（G29・D-60）。改行区切りの JSON-RPC（`rpc`）、ツールの定義（`tools.json`）と版ごとの出し分け・切り詰め（`tools`）、引数の検証（`schema`。JSON Schema の部分集合）、主ループと取り消し（`session`）、`Backend` と `ProjectBackend`（`backend`）、パスの閉じ込め（`path`）、`tz_check`・`tz_fix_plan`（`check`）、知識グラフと参照のツール（`knowledge`）、答えのキャッシュ（`cache`）、資源（`resources`） |
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
| f16 | AArch64 は LLVM の `half` 命令（FEAT_FP16 のない CPU でも基本命令で、演算ごとに 1 回だけ丸める）。x86-64 は `--cpu native` のときだけ、F16C で変換、AVX512-FP16 で演算と変換。WASM とそれ以外はソフトウェア。結果のビットは全経路で同一（D10）。速度の実測は macOS arm64 だけ。結果のビットの照合は、CI の runner（macOS arm64・Linux x86-64・Linux arm64）で実行している。Linux x86-64 は runner の機能に応じ `conversions|hardware` を許容し、現在までに `conversions` を確認済み。AVX512-FP16 の `hardware` と Windows の実行は未確認 |
| SIMD | `-O3` におけるループおよび SLP 自動ベクトル化。メモリ上で連続した配列配置、型の特殊化、不要なコピーの徹底排除により、LLVM が最適化しやすい IR を生成。`--cpu native` 指定時にはビルドホスト固有の命令セットを有効化 |
| 移植性 | デフォルトの `--cpu generic` ではターゲットアーキテクチャのベースライン命令セットを採用。同梱の i64 配列和のみ実行時 ISA 選択を実施。`native` 指定時は配布バイナリの前提条件にビルド機の ISA が含まれる |
| 複数 CPU コア | `Task.parallel` 向けに遅延起動する常駐スレッドプールを実装。ハードウェアの CPU コア数に応じて追加スレッド数を適切に制限し、呼び出し元スレッド自身も自グループの処理を推進。WASM は逐次フォールバック。自動並列化は未実装 |
| プロファイル誘導最適化 | 実装済み（PR07）: `--profile-generate DIR`・`--profile-use PATH`（`build`・`run`。native の `exe`・`object`、wasm32 の module）。Clang が IR を機械語にするときに `-fprofile-generate`・`-fprofile-use` を足すだけで、Tsuzuri の IR・丸め・NaN・符号付きゼロ・トラップは変わらず、旗なしの成果物とキャッシュのキーも同じ。効果は種目で正負が分かれる（control: `closure_*` 12〜17% 速く、`tail_*` 約 20%・`match_dispatch` 約 63% 遅い）ので既定にせず、測った値は [benchmarks](benchmarks.md) の PR07 の節にある。計装のカウンターは原子的でなく、並列の Task の回数は少なく数える（原子的にすると計装版が約 60 倍遅く、付けない）。計画のみ: ThinLTO との併用、Windows、Linux・x86 での確認、wasm の間接呼び出しの値 profile、trap の分岐の重み（走査で根拠がなく取り下げ）、数値・数学のランタイムの計装（効果が測れず取り下げ） |
| コンパイラ自身（フロントエンド） | 構文解析・関数ごとの本体の型検査と仕上げ・所有権の検査を scoped thread で並列に実行（PB04）。上限は `TSUZURI_THREADS`（CLI は未設定のとき使える core 数の最大 8、ライブラリは 1）。診断・IR・キャッシュの bytes は thread 数で変わらない。specialize・closures・emit・読み込みは逐次。macOS の実測は [benchmarks](benchmarks.md) の PB04 の節にあり、Linux と Windows での速度は未測定 |
| GPU | 実験的なカーネル抽出、CPU 参照実装、厳密な整数 WGSL 出力、名前で選ぶ緩い `f32`・`f16` の WGSL 出力（`Gpu.map_relaxed`・`--emit wgsl-relaxed`。CPU 上の評価は厳密のまま）、`Gpu.request Gpu.WebGpu` による WebGPU 上の実行（native は wgpu-native の動的読み込み、WebAssembly は `--wasm-feature webgpu` と JSPI のホスト）、WebGPU ホスト試作、SPIR-V の出力（`--emit spirv`・`--emit spirv-relaxed`。厳密な `i32`・`i32u`・`i64`・`i64u`・`f32`）、`Gpu.request Gpu.Vulkan` による Vulkan 上の実行（native だけ。Vulkan のローダーを動的に読み込み、厳密な `f32` は float controls の報告と組み込みの適合プローブを通るデバイスだけ）、呼び出しごとに測ったコストで CPU 参照と Vulkan を選ぶ `Gpu.Auto` を実装。WebGPU での厳密な浮動小数点と 64 bit 整数、GPU 常駐バッファ、自動オフロード、Vulkan の離散 GPU・Linux・Windows での検証は未実装または未確認 |
| WASM | bulk-memory に標準対応。SIMD128、マルチスレッド（threads）、`--wasm-host wasi`、Async の JSPI はそれぞれ明示的なオプトイン（opt-in）制。デフォルトは SIMD なし・ホストインポートなし（IO は `tsuzuri_io`）・逐次実行であり、OS API の呼び出しは `E2000` で拒否 |
| 協調的な非同期計算 | `std/Async.tc` の継続と `Step` で `Async.run`、native の `Async.block_on`、ホスト駆動の `Async.start` を実装。native の reactor は per-thread mailbox、WASM の `block_on` は JSPI を使う |

B08 の測定（Apple M1 Max、macOS、`-O3`、9 回）は、末尾の `return!` で 100 万回 `yield` する計算が中央値 0.18 秒、最大 RSS 約 1.7 MB でした。非末尾の `let!` と `yield` を 1000 / 2000 / 4000 段重ねた計算は、それぞれ中央値 0.03 / 0.11 / 0.40 秒でした。これらは合否の閾値ではなく、継続の確保と非末尾再開のコストを記録した値です。条件・raw samples・再現方法は [性能測定](benchmarks.md#async-の中断と継続のコストb08) にあります。IR の `bind_step` 特殊化では、代表的な `i64` 結果の本体に `@tz.alloc` が 4 箇所、継続環境の clone helper に 2 箇所あり、同じ継続を再利用する最適化はまだ行っていません。

新しい組み込み関数や標準ライブラリを設計する際は、要素ごとの汎用的な関数呼び出しを基本実装とせず、要素型・メモリ連続性・データサイズを静的に把握できる一括操作（バルク操作）として設計してください。必ず厳密で正確な基準実装（リファレンス）を用意し、スカラー、SIMD、並列 CPU、GPU の各経路で同一の契約が満たされているかを検証します。既存の所有権・借用・不変性の情報から別名関係（エイリアス）の不在や独立性を論理的に証明できる場合にのみ最適化を適用し、根拠のない `noalias` 属性などを安易に付与してはなりません。

文字列の連結では、リテラルだけの木を一つの定数に畳み、長さだけを使う場合は overflow とサイズの検査を残して結果の確保を省きます。
PM06 の追加スタック領域は入口ブロックに置き、連結 1 箇所 256 bytes、環境 1 KiB 以下、関数あたり合計 4 KiB です。
Task 本体と、関数・閉包の参照グラフの循環に入る関数では使いません。`Frame::Buffer` は結果の buffer と空でない長さ、
`Frame::Environment` は記述子の環境 pointer をアドレス比較して識別し、移動時にはヒープへ移送します。
環境のスタック解放は捕捉を宣言順で drop し、環境自体は free しません。
PM07 の `append` は左辺の frame が空のときだけ使用します。静的・スタックの文字列を `realloc` へ渡さず、
左辺の所有権を `append` へ渡した後は二重に drop しません。文字列の記述子と容量の保証は変更しません。

PR03 の戻り値 slot は、再帰しない tagged union（`Maybe`・`Result` など。payload のない enum、payload そのものである niche union、slot が 1 KiB を超える型を除く）を返す関数に、`ret` が 2 つ以上あり、そのすべてを slot にできるときだけ使います。
入口の alloca `%tz.return.slot` へ store し、`tz.return:` ブロックで load して 1 回だけ `ret` するので、inline 後に呼び出し側の aggregate phi が残りません。
slot は `scopes` にも frame にも入れず、drop・移動の経路を変えません。
slot にできる exit は、値を作る exit（constructor）と、記憶域や変更しない引数から読む exit（参照外し・static・引数、引数の欄・添字・payload）だけです。
呼び出しの結果を返しうる exit（呼び出し・パイプ・引数でない局所、それらの欄・添字・payload、その他の形）が 1 つでもある関数は、呼び先にかかわらず全 exit を平文の `ret` のまま残します。
LLVM は `ret (call f)` を sibling call やループにし（native `-O2` 以上）、union を返す相互再帰、局所を経由する再帰、引数で渡した閉包を経由する再帰が 1e8 段でも定数のスタックで走ります。
slot はこの呼び出しを隠し、閉包を引数で渡す再帰を含め、呼び先が循環に戻らないことを呼び出しグラフから証明できないので、呼び出しを返す関数には使いません。
一部の exit だけを slot にしても `ret` が複数残って phi が消えないため、1 つでも保つ exit があれば全 exit を保ちます。
`ret` の位置は最終の行から探します（`facts::apply` が本体の前に scope の宣言を挿入しても動き、`ret` の数が `emit_tail` の報告と合わない関数は書き換えません）。

`llvm::last_use::LastUses` は具体化 IR の後方生存解析で、所有権チェッカーの State／loan は参照しません。
ループと match の失敗先は固定点で扱い、訪問 100,000 回・深さ 128 を超えると結果全体を空にします。
生成と `copies::sites` は同じ解析を使います。全体の Local は単一使用または最後の使用、
R2 の Field 根は単一使用だけを使います。ガードとパターン手順は subject の別名が生きているため不透明です。
束縛の move はガード成功後だけで、利用者 Drop を経由しない非重複の所有射影に限ります。
`tests/last_use_fuzz.mjs` は独立した値セマンティクスの AST 評価器と native の `-O0`／`-O3` を照合します。

バックエンドの自動選択にあたっては、メモリ確保、コンパイル・初期化、ディスパッチ、データ転送、同期、および結果の回収に至るすべてのオーバーヘッドを含めた損益分岐点（閾値）を、実機上で計測して決定します。小規模な処理を漫然と GPU にオフロードしたり、スレッド数を増やせば無条件に高速化すると仮定したりしてはいけません。GPU 常駐データについてはホスト・デバイス間の往復転送を避ける設計を徹底します。また、GPU 等に対応していない環境でも言語契約を維持できる CPU フォールバック経路を提供し、実際に選択された実行経路を観測可能にします。GPU 実行を明示的に要求したにもかかわらず環境が未対応である場合や実行に失敗した場合は明確に診断エラーを報告し、暗黙のうちに成功扱いにしたり結果の精度を改変したりしてはなりません。`Gpu.Auto` がこの方針の実装で、規則の定数は実機の測定から作り（1 台のマシンの経験則）、選んだバックエンドは `Gpu.last_backend ()` と `TSUZURI_GPU_DEBUG` で観測できます（[性能測定](benchmarks.md#vulkan-と-gpuautof09-phase-3)）。

高速化を目的として、整数のオーバーフロー（折り返し）、飽和演算、最近接偶数丸め、NaN の扱い、符号付きゼロ、式の評価順序、トラップ挙動を変更することは禁止します。浮動小数点の集約順序の変更や FMA（積和演算）の融合が必要な場合は、まず独立した別名 API や明示的なモードとしての契約を策定し、既存の演算子へ暗黙的に適用してはなりません。新設した最適化経路には、境界値テストおよび参照実装との照合テストを用意し、同一条件下での C/C++ との比較検証を実施してください。共有 CI では意味論の正しさとリグレッション（性能退行）の有無を検査し、厳密な性能閾値の判定はノイズの少ない専用環境で行います。

WebAssembly の機能フラグ（WASM feature）は `BuildOptions.wasm_simd`、`wasm_threads`、`wasm_jspi`、`wasm_net`（E09。`jspi` が必要で、threads・WASI・wasm64・native とは排他）、`wasm_tail_call`（PM09。保証された末尾呼び出しを `musttail` にする）で表現されます。ドライバーは SIMD 有効時に `-msimd128`、デフォルトでは `-mno-simd128` を、tail-call 有効時にモジュール本体の Clang へ `-mtail-call`、デフォルトでは `-mno-tail-call` を渡します。生成される LLVM IR には feature 要件がコメントとして記録されます。`tests/wasm_simd.mjs` は `llvm-objdump` の逆アセンブル結果と `BigInt` 参照実装を用いて検証を行い、即値の `0xfd` を SIMD オペコードと誤認しないように厳密に命令解析を実施します。

マルチスレッド対応（threads）は WASM およびオブジェクト出力専用の機能です。C11 の freestanding な `task-wasm-threads.c` を `-matomics` および `-mbulk-memory` でコンパイルし、`wasm-ld` の `--shared-memory` および `--import-memory` を用いて結合します。`heap-wasm.ll` 内のアロケータを内部名へ変更し、`heap-wasm-threads.ll` のロックラッパーから呼び出す設計とすることで、`realloc` 内部で行われるメモリ確保と解放も単一のロック内で安全に完結させています。`tsuzuri_thread_heap_live_bytes` は、ヒープの長さから空きブロックとキャッシュのブロックの容量を引いた、使用中のバイト数を返します。
共有状態はリニアメモリ上に配置され、`wasm-ld` による一度限りのデータ初期化を利用します。各インスタンスの `__stack_pointer` とスタック範囲（`tsuzuri_stack_base`、`tsuzuri_stack_top`。メインスレッドは 0）のみをホスト側から設定します。
タスクグループは呼び出し元のスタック上に確保されます。次の添字（`next`）は `queue_mutex` なしに `fetch_add` で 1 つずつ取り、`length` は公開後に変わりません（native と同じ形です）。worker は `join_one` で、join のときだけ `queue_mutex` の下で最初の添字を取り、残りは lock なしに取り尽くして、走らせた数を `remaining` から 1 回だけ引きます（添字ごとに lock を 2 回取ることはありません）。失敗は cold path で、最小の添字を記録し、`next` を `length` へ進める compare-exchange 1 回で未着手を外します。引いた後、スレッドはグループを参照せず、呼び出し元はロック内でアンリンクして復帰します。要素が 1 つのグループは公開せず、その場で走らせます。
呼び出し元スレッド自身もタスクの実行を進めます。グループを公開する前に最初のアイテムを自分のものにし、終わりを待つあいだは自グループのアイテムだけを走らせます（native と同じです。ほかのグループのアイテムは、ワーカーが古いグループから取るほか、チャンネルを待つスレッドが手伝うだけで、結合を待つスレッドは取りません）。エポックベースの wait/notify によってアイドル待機を行い、ヒープのロック内ではユーザーコールバックを決して呼び出さない設計としています。
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

`--emit bitcode`（PR08、`src/driver/lto.rs`）は、ホストの ThinLTO のリンクが公開関数を自分のループへ inline できる、ライブラリの bitcode を出します。`Entry::Library` の IR（枝刈りなし）に、次の手順で足します。(1) `probe_target_attributes` が、小さな C の関数を同じ Clang と同じフラグ（`compile_args`、`--cpu native` なら `native_ir_cpu_args`。wasm32 の `--wasm-feature simd128` は `--target=wasm32-unknown-unknown -mbulk-memory -msimd128`）で `-S -emit-llvm` にして、`target-cpu`・`tune-cpu`・`target-features` を読みます。(2) `stamp_target_attributes` が、IR の全 `define`（`}define` のように前の関数の閉じ括弧と同じ行に続くものを含む）に属性 group を足します。既定は `target-cpu` と `tune-cpu` だけで、`--cpu native` と wasm32 の simd128 は `target-features` も足します。`"target-features"` を自分で持つ `define`（F08 の `@cpu` 版）は group でそれを上書きされるので、cpu だけの group を使って features を保ちます。既存の group（`numeric.ll`・`math.ll`）は、鍵を置き換えて自分の features を保ちます。結果は決定的で、2 回かけても変わりません。(3) C ランタイム（`task.c` として 1 つに連結したもの）は、`-S -emit-llvm` で IR にして（`-c` は付けない）、`llvm-link --internalize` で、プログラムの IR を先頭に 1 つにまとめます。先頭のファイルの記号は外部のまま、ランタイムの記号は内部になり、公開 ABI だけが見えます。ただし公開 ABI の記号のうち、プログラムの IR に定義がなく C ランタイムだけが定義するもの（`shared_exports` の一覧では、`Async.block_on` に届くときの `tsuzuri_async_post` だけです。ほかは IR が定義します）は、そのままでは内部になります。`--internalize` が残す名前の一覧は `@llvm.used` だけで（配布物に `opt` はありません）、`keep_runtime_exports` がその記号の `declare`（IR がすでに宣言していれば足しません）と `@llvm.used` の一覧を、まとめる前の IR に足します。`llvm-link` の後に `check_kept_exports` が、まとめた IR（`combined.ll`）でそれらが内部・private・hidden になっていないかを検査し、なっていれば `E2002` です（`@llvm.used` を読まない `llvm-link` への備えです）。`llvm-link` の探索は `lto::llvm_link_tool`（環境変数、配布物、Clang と同じディレクトリ、PATH）で、必要なのは C ランタイムを使うときだけです。(4) `clang -x ir -flto=thin -c` が bitcode にし、`check_bitcode` が先頭の magic（`BC C0 DE`、Darwin の wrapper の `DE C0 17 0B`）を検査します。LTO の指定を落とす Clang や launcher は `E2002` です。同梱の `scripts/toolchain/clang.rs` は、`-x ir` の経路で `-flto*` を同梱の LLVM Clang へ渡します。C ランタイムは `compile_runtime_object` の PB01 の object やキャッシュを使わず、`native-c` のキャッシュも使いません。bitcode 自体もビルドキャッシュの対象外です（キーが `llvm-link` の同一性と、IR へ併合するランタイムを持たない）。wasm32 は C ランタイムも `llvm-link` も使わず、`wasm.ll` と `with_wasm_heap_limit` までは `--emit object` と同じ IR で、`--wasm-feature simd128` のときだけ属性を足します（足さないと、リンクの LTO の code generator が SIMD を落とします）。bitcode を読む LLVM は、書いた LLVM と同じか新しくなければならず、コンパイラは版を検査しません（Apple clang の版数は LLVM の版と一致しないため）。`tests/lto.rs` が、stamp の単体、全 fixture と `examples/*` の IR、C・C++ のホストの inline、C ランタイムの結合と隠蔽、`--emit shared` の export 一覧との一致、ホストのスレッドからの `tsuzuri_async_post`、`-g`・`--cpu native`・`-O0`・`-Oz`・`--trap-info`、CPU kernel・allocator・trap return の往復、決定性、Clang と `llvm-link` の失敗（wasm32 には `llvm-link` の案内が出ないことを含む）、wasm32 の `wasm-ld` と SIMD を検証します。測定は `benchmarks/run-lto.mjs`（[benchmarks.md](benchmarks.md#pr08-言語間-lto-と-bitcode-出力)）です。
ネイティブ環境では、`driver::probable_stack_exhaustion` が子プロセスの終了シグナル（SIGSEGV、SIGBUS）を検知してスタック枯渇の可能性を推定し、`E2005` またはテスト失敗の理由として報告します。`tests/trap_boundary.mjs` により、`-O0`／`-O3` における 17 のケースが検証されています。

ネイティブオブジェクトにおけるエラー復帰境界（E14 Phase 2）は、`--trap-mode return` によって有効化されます。これはネイティブ出力（object、llvm、header）でのみ受け付けられ、`--trap-info` を内包します。各エクスポート関数 `tz_name` に対して `int32_t tsuzuri_try_name(tsuzuri_trap_info *trap, 結果ポインタ, 引数...)`（ステータス 0: 成功、1: トラップ、2: 入れ子呼び出しエラー）が追加生成され、C ヘッダー（型定義の重複は `TSUZURI_TRAP_INFO_DEFINED` で防止）および IR のサンクが出力されます。
`setjmp` は `src/runtime/trap.c` の内部でのみ使用され、LLVM IR には一切現れません。`llvm_traps::instrument` が `tz.trap.report` の先頭に `tsuzuri_trap_raise(site, kind)` を挿入し、境界のスレッドローカルフレームが存在すれば `longjmp` で脱出します。境界が存在しない場合は従来どおり標準エラーへ報告したのち `llvm.trap` で終了します。
この境界下では、`heap-native.ll` 内の `malloc`、`realloc`、`free` が `tsuzuri_tracked_*` に置き換えられます。返されるポインタ自体は通常の `malloc` と同一ですが、確保の追跡情報は境界ごとに独立した双方向リストおよび拡張ハッシュテーブルで管理され、解放時にはこのテーブルから対象ポインタを検索します。追跡情報のメモリ確保に失敗した場合は獲得したポインタを解放して NULL を返し、`realloc` の失敗時には元のメモリ領域を保護します。トラップ発生時には追跡対象の全ブロックが一括して `free` されます（Tsuzuri コードは巻き戻らないためデストラクタ呼び出しは行いませんが、ランタイムはコールバック呼び出し前にロックを手放す規約となっています。`Mutex.with_lock` の中のトラップだけは、境界が下記の `tsuzuri_sync_hooks.abandon` でそのスレッドの持つロックを解放します）。正常終了時には追跡情報のみが破棄され、ホスト側が所有する結果バッファはそのまま残されます。
境界管理構造体（ロック、リスト、ハッシュテーブル）は `tsuzuri_boundary_run` の自動変数（ローカル変数）としては保持せず、追跡対象外の通常の `malloc` で確保して `volatile` ポインタとして保持します。これは、`setjmp` 以降に変更された非 `volatile` な自動変数が `longjmp` による復帰後に未定義値となり、コンパイラの最適化（`-O2` 以上で顕在化）によってメモリ解放処理が古いポインタを読み出してしまう不具合を防ぐためです。境界構造体の確保に失敗した場合は即座にアボートします。
POSIX ネイティブの通常オブジェクト、`task.c`、および `trap.c` は、弱いシンボル（weak）である `tsuzuri_trap_hooks`（`owner`、`item`、`resume`、`allocate`、`release` の 5 つの関数ポインタ）を共有します。`trap.c` のコンストラクタがこのテーブルを設定し、境界ランタイムがリンクされていない場合は全スロットが NULL となります。公開 API の `tsuzuri_alloc`／`free` もこのテーブルを経由するため、通常オブジェクトと境界付きオブジェクトのどちらを先にリンクしてもメモリ確保と解放の実装が完全に一致し、ホスト側の外部関数が返した所有バッファも境界の追跡機構へ安全に登録できます。未定義の弱い関数への参照は行いません（macOS のリンカーがこれを拒否するためです）。
`task.c` は、フックテーブルの `owner` が存在する場合、タスクグループを投入したスレッドの境界情報を取得し、各ワーカー（および投入スレッド自身）のタスクアイテムを `tsuzuri_boundary_item` の保護下で実行します。アイテム内でトラップが発生した場合はグループ内の最小インデックスとして記録され、以降のアイテム開始は中断されます。全アイテムの終了後、投入スレッドが `tsuzuri_boundary_resume` を通じて境界へ復帰します。トラップが発生したタスクの内部状態は未定義となるため、先行して `Err` を返したアイテムが存在する場合でもトラップが優先して返されます。
`Mutex.with_lock`（F10）の中のトラップは、ロックを保持したまま境界へ脱出します。`task.c` と `trap.c` は、弱く hidden な構造体 `tsuzuri_sync_hooks`（関数ポインタ `abandon` 1 つ）を共有し、`task.c` のコンストラクタがスレッドのロックを解放して `tz_mutex_held` を消す関数を設定します。`tsuzuri_boundary_item` と `tsuzuri_boundary_run` は、トラップを捕捉したスレッドでこれを呼んでから結果を記録します。ロックの語は境界が解放するブロックの中にあり、ロックを待つ同じグループの子どもは、解放により起きて、そのアイテムを終えます。ポイズンはありません（`Mutex` は 1 回の呼び出しの外へ残らないためです）。`Channel` で待つタスクのデッドロック（F10 Phase 2）は、判定が待ちスレッドを起こして `status 2` を返させ、生成コードが assert のトラップを起こすので、境界から見ると普通のトラップです。境界は呼び出しが確保したチャンネルのブロックも解放し、待ちの一覧（スタック上の `tz_waiter`）は判定で空になります。`tests/trap_return_sync.mjs` と `tests/trap_return_sync_host.c` が、object と IR のリンクで、1・2・8 スレッドの構成、`-O0`／`-O3`、残存確保数 0 を確かめます（ロックのトラップ、グループの中のロックのトラップ、デッドロック、端を持つ生産者のトラップ、ロックの中のチャンネル操作）。
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
`@cpu` を付けた利用者関数（F08 Phase 3）は、`FunctionEmitter::emit` が define 行に `"tz-cpu"="<levels>:<関数 id>"` を付け、`src/llvm_cpu.rs` の `multiversion` が trap 計装後の IR を書き換えます。元の関数は `.cpu.baseline` に改名して debug 情報を保ち、level ごとの `.cpu.<名前>` 版は `"target-features"` を付けて debug 情報を外します。元の名前には、関数ごとの `@"<名前>.cpu"` に `tsuzuri_cpu_pick(levels)` の結果を monotonic で cache し、switch から各版を tail call する stub を置きます。stub は元の subprogram を複製した自分の subprogram と呼び出し位置を持つので、portable 版が stub へ inline されても inline 位置が保たれます。版ごとに 256-bit ベクトルを渡すレジスタが異なるため、版の本体から 256-bit ベクトルを含む型で直接呼ぶ関数は推移的に同じ level の版を作り、関数値や extern へ渡す呼び出しは `E1005` にします（型検査も `validate_cpu_functions` で同じ規則を先に検査します）。選べる level は `cpu::host_levels`（`Platform::cpu_levels`。Linux x64 と Linux Arm64）で、それ以外の build 先では属性を外すだけです。
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
呼び出し側は `host_call` を直接発行し、callback extern 自体の関数本体は生成せず、関数値（`%tz.closure`）も作成しません。関数引数の実引数としては `ptr @tz.callback.<修飾名>` が渡され、`llvm_abi::wrapper` が内部用の C ABI ラッパー関数を対象関数ごとに 1 つずつ生成します。
wasm32 ではラッパー関数のアドレスが関数テーブルのインデックスとなるため、IR 内に `@tz.callback.` が存在する場合にのみ `wasm-ld` へ `--export-table` を渡します。コールバックを使用しないプログラムの出力成果物に変更は生じません。

## 不変条件

### Whole-build cache

`cache.rs` はストリーミング形式の SHA-256 実装と NIST テストベクトルを備え、既存の `serde_json` を用いてメタデータをシリアライズします。型検査と IR 生成の途中段階のキャッシュは作成しません（パースの結果は次の frontend cache が持ちます）。
コンパイラ実行バイナリ全体のダイジェストハッシュをキャッシュキーに含めることで、同一の Git コミット上に存在する未コミットの開発版バイナリ同士も厳密に識別します。`build.rs` が埋め込む Git コミット文字列のみに依存することはありません。
キーは長さ付きフィールドで構成され、コンパイラバージョン、ホスト環境、ビルドオプション、実行アクション、すべてのソースファイルおよびマニフェストのパス・内容バイト列・由来（origin）、生成された IR、コンパイラおよび依存ツールのバイナリダイジェスト、ツールの `--version` 出力、および関連環境変数を含みます。
`--cpu native` 指定時には、Clang が定義するターゲットマクロ群もキーに含めます。`--target TRIPLE` の triple はビルドオプションとしてキーに入り、`TSUZURI_SYSROOT` は環境変数としてキーに入るので、target や sysroot を替えたビルドが以前の成果物を再利用することはありません。ファイルパスは Unicode 正規化を行わず OS の表現をそのまま維持し、絶対パスによるソースマップとパッケージの識別性も分離して管理します。
アーティファクト、トラップ情報、DWARF デバッグ情報はステージング領域から復元され、実行権限およびサイドカーファイルを完全に維持した状態で公開処理（`publish_outputs`）へ渡されます。キャッシュのヒット時・ミス時のいずれにおいても、ソースファイル、マニフェスト、およびハードリンクの保護チェックを迂回することはありません。
キャッシュルートディレクトリには専用のマーカーファイルが必須であり、シンボリックリンクは拒否されます。各キャッシュファイルのサイズと SHA-256 ハッシュが検証され、メタデータの欠落、ファイルの破損、または未知のフォーマットを検出した場合は安全にキャッシュミスとして扱います。信頼境界は同一 OS ユーザーのプライベートキャッシュとして定義されます。
保存処理ではキーごとに非待機の排他ロック（create-new lock）を獲得して複数プロセスの重複書き込みを防止し、全ファイルの出力完了後にディレクトリのリネームによってアトミックに配置します。ロック競合が発生した場合はキャッシュ保存をスキップし、ビルド処理をブロックさせません。I/O 障害は警告として報告し、生成されたビルド成果物自体はそのまま保持します。
GC（ガベージコレクション）は最大 4096 エントリまで走査し、最終アクセス日時、合計 2 GiB の容量上限、30 日間の有効期限を基準に最大 128 件ずつ回収します。古い不完全エントリ、残存ロック、一時領域も回収対象であり、上限はソフトリミットです。
macOS のデバッグ実行ファイルとデバッグ共有ライブラリにおける DWARF は出力ファイル名に依存した情報を含むため、この場合に限って出力先パスもキャッシュキーに含めます。Windows のデバッグ実行ファイルは隣の PDB をファイル名で指すので、その名前もキーに含めます（G16）。`--emit shared` はファイル名を install name（macOS）か soname（Linux）として埋め込むので、出力のファイル名もキーに含めます（`hash_output_path`）。それ以外の場合における別出力先へのアーティファクト再利用性は維持されます。
動作は `tests/cache.mjs` により、実際の CLI を用いたキャッシュヒット（ツール起動回数の削減確認）、ミス、破損時の回復、並行書き込み、no-cache 指定、実行権限の保持、トラップ情報／DWARF の整合性、依存関係変更時の無効化が検証されています。

### コード生成単位の object cache（PB07）

`-O0` のビルドは、環境変数 `TSUZURI_CODEGEN_UNITS` で `auto` か `split` を選ぶと、Tsuzuri が出した最終の LLVM IR のテキストを決定的な codegen unit に分け、unit ごとの object を内容で cache し、cache に無い unit だけを Clang で並列にコンパイルして、全 object を 1 回でリンクします。既定（`single`）は分けず、従来の 1 つのモジュールのビルドです。
対象は native の実行ファイルと `--emit object`、wasm32 の `--emit wasm` で、`--trap-info` と `-g` を付けたものも含みます。`-O1` 以上、`--backend fast`、`--emit shared`・`bitcode`・`llvm`、`--profile-generate`・`--profile-use`、`--trap-mode return`、`--freestanding`、wasm の threads・wasm64・スタック検査、Windows のホスト、`--target TRIPLE` のクロスビルド、`test` と `bench` の runner（別の経路です）は、どの値でも 1 つのモジュールです。
macOS の `-g` の object は、再配置可能リンクが DWARF を捨てるので 1 つのモジュールです。`--emit object` は、native の object を局所化する道具（macOS の `nmedit`、ほかは `objcopy`）がない機械でも 1 つのモジュールです。
`TSUZURI_CODEGEN_UNITS` は `single`（未設定と空も同じ。既定）・`auto`・`split` を取ります。`single` は分けず、`split` は対象のビルドをどれほど小さくても分け、`auto` は IR が 16 MB 以上の対象のビルドだけを分けます。分けないのが既定なのは、初回のビルドが長くなるからです。unit の数だけ Clang のジョブ（約 8 ms の CPU 時間）と繰り返す宣言が増え、unit 全体は 1 つのモジュールの 3 倍ほどの CPU 時間を使い、cache には小さい object が数千個入ります。測った機械では、1 つのモジュールの本体を編集したあとのビルドが短くなる分で、初回の分を取り戻すのに合成 400 module で 2 回ほどのビルドが要りました（[性能測定](benchmarks.md#pb07-codegen-unit-による増分ビルド)）。ほかの値は `E2000` です。
この環境変数はコンパイラの動作を選ぶだけで、成果物の意味は変わりません。whole-build の cache の鍵には、`auto` か `split` のときだけ `codegen-units` の欄（値そのまま）が入り、未設定・空・`single` の鍵は PB07 より前のものと同じです。

分割は IR のテキストだけを入力にします（`src/llvm_split.rs`。emitter は変えません）。`build_complete` が IR に `without_closure_inlining`・`windows_abi`・`with_no_red_zone` と wasm の `with_*` を適用し、`runtime/wasm.ll` を足し終えたあと、whole-build の cache の照会（従来のまま。当たれば何も走りません）に外れたときに、Clang を呼ぶ直前で分けます。

- `tz.fn.<module>.<name>` と、その `tz.apply.`・`tz.env.clone.`・`tz.env.drop.`、ファイルごとのトラップ報告 `tz.trap.report.<module>` は unit `m.<module>` に入ります。ランタイムと、型の名前で付く補助関数は、名前の hash で選ぶ 16 個の bucket `g.0`〜`g.f` に入ります。生成された関数（ラムダ、ジェネリック関数の具体化、特殊化）は、それを使う関数がすべて 1 つの unit にあるときはその unit に、複数にあるときは名前の hash の bucket に入ります。使う側から先に置くので、ラムダの中のラムダは外側に付いていきます。そのため、あるモジュールの本体の編集で変わる unit は、そのモジュールの 1 つです。
- 通し番号を含む名前（`$lambda.12`、`$mono.3`、`tz.specialized.4`）は、ほかの部分の変更でずれるので、関数自身のテキストの hash で付け替えます。呼ぶ関数の名前を先に新しい名前にし（強連結成分の順、callee が先）、その名前で hash するので、名前が変わるのは関数か呼ぶ先が変わったときだけです。テキストが同じ生成関数は 1 つの関数に統合します（関数値は比べられないので区別できません）。アダプタ `tz.apply.<symbol>.<index>` は、LLDB の整形が名前から関数を引くため、関数の新しい名前に `.<index>` を付けた名前にします。
- 番地の意味を持たない `private`・`internal` の定数（文字列リテラル、クロージャの記述子、トラップのメッセージ）は、使う unit ごとに `@tz.unit.<k>` としてコピーします。メタデータの節と属性グループもコピーして番号を付け直します（unit ごとの DWARF の compile unit は、使う節から決まります）。定数でないグローバル変数と `weak` の定義は所有する unit を 1 つにし、ほかの unit は `external hidden` で宣言します。`internal` だった定義は `hidden` にし、リンク内の呼び出しは通しつつ公開 symbol は増やしません。ただし `hidden` の定義も object の大域 symbol なので、ホストの object やライブラリが同じ名前（ランタイムを埋め込むビルドの数値ランタイムの補助関数 `power`・`decode`・`store` など）を定義していると、1 つのモジュールでは local だったのに、リンクが `duplicate symbol` で失敗します。そこで、`hidden` にする定義のうち名前が `tz.` で始まらないもの（emitter の名前は `tz.` で始まり、C の名前にはありません）は `tz.rt.<名前>` に付け替えます。付け替えは定義・宣言・参照のどの unit でも同じで、その名前だけから決まり、ほかの unit が使うかどうかには依りません（unit のテキストはほかの unit に左右されません）。付け替えた名前がテキストにすでにあれば `SplitError` で、1 つのモジュールで行います。
- 最上位に知らない形があると `SplitError` で、そのビルドは 1 つのモジュールで行います（常に正しい経路）。

unit の鍵は、unit のテキスト、Clang の引数（target、最適化、`-g`、CPU・SDK の旗）、Clang の識別（実行ファイルの内容と版）の SHA-256 です。コンパイラ自身の識別は入りません（IR が変われば unit のテキストが変わるためです）。ソースのテキストやパスは、unit のテキストの中にあるもの（トラップのメッセージと DWARF）以外は鍵に入りません。
object は同じ保存先の `units/` に、成果物の cache と同じ `BuildCache`（マーカー、`0o700`、原子的な rename、大きさと SHA-256 の検証）で入ります。保存先を分けるのは、小さい object が数千個になっても成果物の cache の掃除に影響しないためです。上限は 2 GiB・30 日で、走査は 32,768 エントリまで、1 回の掃除は 1,024 件までです。掃除は保存したビルドのあとに 10 分に 1 回まで、最後に使った時刻の更新も 10 分に 1 回までです。cache に入れるのは object だけで、構文解析・型検査・IR の途中の cache は作りません。

cache に無い unit は、テキストの大きさが等しくなるように組み分けして、少数の Clang プロセスで並列にコンパイルします（`clang -c a.ll b.ll …`。プロセスの数は使える core 数で、`TSUZURI_THREADS` があればその値です）。Clang の出力は組の順に集め、警告のあるものは cache に入れません。cache の読み出しは、全 unit の鍵の計算と読み（SHA-256 の検証を含みます）を先に終えてから、見つけた object と Clang に渡す `.ll` を書き、コンパイルのあとも、全 object を読んでから entry を書き、最後に最終使用時刻の更新を書きます。macOS では、あるプロセスの別のスレッドがファイルを作っているあいだ、そのプロセスの `open` が 1 桁遅くなり（この機械で 1,000 module の初回のビルドの `units` が 14 s になりました）、読みと書きを分けると 2 s 台になるためです。リンクは `objects.rsp` の response file に unit の順で書いた object を、単一のモジュールと同じ最終リンクの引数（`dead_strip`、`-Wl,-u,_tz_<export>`、`--strip`、リンク入力、リンカーの選択、`-pthread`）で 1 回行います。ランタイムの object（PB01）は 1 つのモジュールのときと同じものを加えます。
macOS の `-g` は、リンクのあとに `dsymutil` が同じ object を読むので、object は一時ディレクトリに置いたままにします。native の object は `clang -r -nostdlib [-Wl,-keep_private_externs]` で unit の object（と C ランタイム）を 1 つにしたあと、unit が `hidden` にした定義だけ（もとは `internal` のもの）を局所化します（`tz.rt.` へ付け替えた定義は、その新しい名前で指定します）。複数の Tsuzuri の object を 1 つのプログラムにリンクしても名前が衝突せず、ホストから見える大域 symbol が 1 つのモジュールのものと同じになるようにするためです。wasm32 は同じ wasm の旗で unit をコンパイルし、`wasm-ld` が runtime の object と unit の object をリンクします。旗は 1 つのモジュールと同じ関数（`wasm_target_args`）が出すので、`--wasm-feature tail-call` を付けたときは、すべての unit に `-mtail-call`（付けないときは `-mno-tail-call`）が付き、runtime の object には付きません（runtime の object は独自の引数の一覧で、同梱の object は引数の完全一致で選ぶためです）。unit の鍵に Clang の引数が入るので、旗の違うビルドは同じテキストの unit でも object を共有しません。保証された末尾呼び出し（PM09）の `musttail` の呼び出しは、関数どうしが別の unit にあっても保たれます。呼び出しの直後は値を返す `ret` のままで、別の unit にある呼ばれる側は、定義と同じ結果と引数の型（ABI の属性を含む）で宣言されます。`llvm.lifetime.start`・`end` の宣言は、それを呼ぶ unit に 1 つずつ複写します。module の import の並びは unit の object のリンクの順に従うので、単一のモジュールのものと違うことがあります（import を名前で渡すホストには影響しません。`tests/async.mjs` は import の並びではなく集合を確かめます）。
`TSUZURI_TIME_PASSES` の段は `split`（分割。呼び出しスレッド）、`units`（鍵・cache の読み書き・ファイルの書き出し）、`tool.<clang の stem>`（Clang の待ち）です。

`--trap-info` は、分けるビルドだけ、トラップの site の番号を `1 + 種類の数 * (block * 2^14 + place) + 種類` にします（`TrapNumbering::Stable`。種類の数は `TrapKind::ALL` の 18。分けないビルドの番号は従来の通し番号のままです）。block はソースファイルのパスの hash を block の数で割った余り、place はファイルの中の範囲（開始と終了）の hash で、衝突は決まった順の線形探査で解きます。ファイルごとに報告の関数（と、そのファイルのメッセージ）を作ってそのファイルの unit に入れ、ブロックで選ぶ `@tz.trap.report` と `@tz.trap.write` だけが bucket に入ります。ランタイムのトラップの位置は、入口の関数の範囲に固定した場所（そのファイルの最初の place）です。別のファイルを編集しても、あるファイルの site の番号と unit のテキストは変わりません。block の数は、どの番号も正の `i32` に収まる最大の数（`i32::MAX / (種類の数 * 2^14)`。18 種類で 7,281）です。WASM の `tsuzuri_trap_site` は `i32` を返し、ホストの JavaScript はそれを符号付きの数として位置表の `id` と比べるからです。範囲の数が 7,281 ファイルか 1 ファイル 16,384 か所を超えるときは従来の番号で、1 つのモジュールです。`<出力>.trap.json` の位置表は、コードに書いた番号と同じ番号から作ります。

出力は決定的です。unit のテキスト、鍵、object、リンクの順は、`BTreeMap`・`BTreeSet` だけで決まり、同じ入力から同じ object になります（macOS のリンカーが実行ファイルに入れる UUID と、`-g` のときのデバッグマップの object のパスは、1 つのモジュールのビルドでもビルドごとに変わります）。
`tests/codegen_units.mjs`（`TSUZURI_CODEGEN_UNITS=split` で、単一との差分、決定性、cache からの組み立て、編集で変わる unit の数、破損した項目、適用条件、公開 symbol、並行ビルド、`--trap-info`・`-g`・wasm32・object、`examples/`）と、`src/llvm_split.rs`・`src/driver/units.rs` の単体テスト（`examples/`・`benchmarks/`・`tests/fixtures/` のプログラムの分割を含む）が確かめます。

### 事前ビルドしたランタイム（PB01）

通常の native 実行ファイル（Windows を含む）と wasm32 モジュールは、
数値ランタイムと大きな数学関数を別 object にして再利用します。`src/runtime_cache.rs` は numeric 宣言の抽出、
math の standalone 化、正確な参照記号集合、同梱 object の照合を担い、sqrt／fabs は caller IR に残します。
`src/cache.rs` が実際のツール・source・flags・CPU・environment を鍵に含め、検証した object を原子的に公開します。
runtime identity はコンパイラ実行ファイル自体に依存せず、選択した UTF module の IR と `cpu.c` の実際の内容を含みます。配布 manifest はコンパイラ自身と runtime object 群を除く同梱リソースの fingerprint も保持し、実行時には manifest の一覧だけに頼らず同梱ディレクトリ上の全ファイルを列挙して内容を検証します（`src/bundle_identity.rs`。ハッシュは並列）。この結果は同梱 Clang・Zig の SHA-256 とランチャーの `--version` の出力と合わせて `<cache>/bundle/` に控え、配布物の署名（最上位と `bin`・`lib`・`zig` 直下のファイル、`manifest.json`、2 階層目までのディレクトリの、大きさ・更新時刻・状態変更時刻・inode）が変わらない間は再利用するので、毎ビルドの全ファイル読み込み（約 760 MB）はありません。ツールの入れ替え、署名の対象のディレクトリへのファイルの追加・削除、配布物の展開し直しは署名を変え、次のビルドが全体を検証し直します。SDK／header などの変更・追加・削除があれば配布 object を拒否し、runtime cache も新しい内容 fingerprint に切り替わります。署名が見ないのは、それ以外の場所での既存ファイルのその場の書き換えと、3 階層目より下の追加・削除で、`<cache>/bundle/` を消すと検証し直します。`TSUZURI_LLVM_CLANG`／`TSUZURI_ZIG` の上書きと配布物でないツールは控えを使わず、従来どおり毎回読みます。
選択順はリソースとツールが一致する配布 object、`<cache>/runtime/`、埋め込みソースからのコンパイルです。
WASM threads の `task-wasm-threads.c`、WASI ホストの `os-wasi.c`（と `arguments.c`）、`-g` の実行ファイルのスタック検査 `stack.c` は配布 object を持たず、`<cache>/runtime/`（単位は `wasm-threads`・`wasi`・`stack-debug`。キーは他の runtime object と同じ）と埋め込みソースからのコンパイルだけを使います。
配布物には numeric／math × native・scalar WASM・SIMD WASM × O0〜O3 の 24 個と、
task／cpu／io の 7 組 × O0〜O3 の 28 個、合計 52 object を含めます。
ツールの上書き・未収録の組・破損は source fallback です。`--no-cache` は配布 object も runtime cache も使いません。
LLVM／header／object／shared、debug／trap 計装、WASM threads・memory64・stack-check の numeric／math は埋め込みを維持します。
外部 runtime は section 単位で未使用部分を除去し、公開 export は保持します。native の実行ファイルは、ランタイムを埋め込む形式（`--no-cache`・`--trap-info`）でも同じ除去を行います（`-g` は除く。PM08）。generic 数値 dispatch の到達する枝は残ります。
整数の表示（`to_string`・`Display`・補間・トップレベルの式の結果）、書式指定付きの補間（`{n:x}`・`{n:X}`・`{n:o}`・`{n:b}`・`{n:+}`）、整数の `Parse.parse` は `numeric.ll` ではなく、モジュールに連結する `runtime/integer.ll` が書き・読みます。
整数だけを扱うプログラムは数値 object を連結せず、native の `examples/hello` は 50,664 から 33,960 byte に、wasm32 の同じプログラムは 35,714 から 6,860 byte になりました（PM08・PM05 の WASM ヒープと PM02 の関数値の表現の後。測定は `docs/benchmarks.md`）。整数を `Parse` し書式指定する `benchmarks/size/parse` は、native の `__text` が 30,904 から 9,156 byte に、wasm32 のモジュールが 76,023 から 16,762 byte になります。
float・decimal の表示・精度つきの書式指定・`Parse` は `numeric.ll` のままです。`runtime/` の掃除は成果物／frontend cache と独立します。
`scripts/toolchain/runtime.mjs` と bundle／archive／smoke は生成・manifest・移設後の消費・破損 fallback を検証します。

### Async の不変条件（B08）

`Async<T>` の式は `Checker::eval` で loan を持たないことを検査します。これは `Async { ... }` の cold な開始、`let!` / `do!` の継続、`Async.start` の実行器表のいずれでも、字句スコープを越えて借用を保持しないための共通境界です。std の `Async.Async` は opaque かつ非 Copy で、`Async.Step` の継続は `Async.__resume` が一度だけ move して呼びます。

`Async.run` は呼び出しごとに独立した仮想時計を持ちます。`Async.start` と `Async.block_on` は、`Async.__take` / `Async.__put` が管理する実行器状態を呼び出しスレッドごとに保持します。native の `Async.__next_id` は操作 ID の上位 bits にスレッド index を埋め込み、`src/runtime/async.c` の `tsuzuri_async_post` はその mailbox へ完了を配送します。`tsuzuri_async_complete` と poll の再入、未知・二重・取消済み ID は trap です。

native reactor は操作を開始時に登録し、完了・取消時に退役させます。`tsuzuri_async_post` は未登録・二重・退役済みの操作を確保前に拒否して 0 を返し、受理した完了だけをキューへ置いて 1 を返します。取消と競合して届いた完了は退役時に取り除きます。mailbox とキューは `tsuzuri_alloc` / `tsuzuri_free` を使い、最後の登録操作がなくなると解放します。host/counting allocator と解放追跡の対象から外れません。時計・条件変数の失敗は診断を出して終了します。`src/runtime/async.c` は POSIX（pthread の mutex・条件変数と `CLOCK_MONOTONIC`）と Windows（`SRWLOCK`・`CONDITION_VARIABLE`・`QueryPerformanceCounter`）を、`tz_async_ready`・`acquire`・`release`・`notify`・`wait_signal`・`wait_timeout` の薄い層で分けており、登録・退役・待機の規則は 1 つです（D-43）。待ちは起きるたびに単調時計を読み直すので、Windows の約 15.6 ms のタイマー分解能は遅れとしてだけ現れ、早く起きることはありません。対応は `Platform::reactor`（Windows x64/Arm64・Linux x64/Arm64・macOS Arm64）です。ほかの native target は引数解析で `E2000` です。runtime を埋め込む Windows の COFF `--emit object` は、task・CPU・IO と同じく非対応です。build と test / debug-test / bench は同じ対応フラグと診断を共有します。ソケットの多重化は E09 の範囲であり、この実行器はタイマーと外部からの完了を待ちます。

WASM は単一スレッドの globals を使い、`tsuzuri_async_set_epoch` が操作 ID の上位 bits をインスタンス世代として設定します。生成グルーは同じ生成モジュール内で世代を再利用せず、古い完了通知を新しい状態へ渡しません。JSPI の待機は `WebAssembly.Suspending`、export は `WebAssembly.promising` を使い、同じインスタンスへの呼び出しを Promise キューで直列化します。渡された typed array は待ち行列へ入れる前にコピーします。WASM threads の executor TLS と trap-return 後の状態復旧は実装していないため、これらとの併用はビルド時に拒否します。

### Frontend cache（G17）

`check`・`build`・`run`・`script`・`test`・`bench`・`doc` は、構文解析の結果をキャッシュルートの `frontend/` に保存して再利用します（`Project::analyze_cached` から `analyze_inputs_with`）。`src/syntax_codec.rs` は `Program` から届くすべての構文型を手書きの `Wire` で符号化します。enum の tag は宣言順の 1 byte、整数は LEB128、文字列と列は長さ付きで、`match` は `_ =>` を使わず全 variant を書くので、構文型に variant や field を足すとここが compile error になります。span の source index は保存せず、復号のときに現在の index を付け直すので、ファイルを足したり消したりして index がずれても再利用できます。
プロジェクトのルート（`fs::canonicalize` した場所）と解析の種類（`program`・`tests`・`docs`）ごとに、パック `p-<project key>.tzp` と manifest `m-<project key>.json` の 2 ファイルだけを読み書きします。パックは、ソースの byte 列、そのソースを読む edition、compiler の同一性から作る parse key ごとの entry（header、`syntax_codec` の符号、SHA-256）を並べたものです。edition を含めるのは、同じ byte 列でも edition によって字句解析の結果が変わりうるためです。ソースごとのファイルにしないのは、ファイルを開く費用（on-access scan のある環境では 1 回 0.3〜1 ms）がモジュールの構文解析より大きくなるためです。manifest はモジュールごとの parse key と interface hash を持ち、前回との比較から `FrontendDelta`（変わったソース、変わった interface、消えたモジュール）を作ります。interface hash は span・文書 comment・関数本体・test・入口式・instance method の本体を除いた符号（`Mode::Interface`）の SHA-256 です。compiler の同一性は形式番号、版、実行ファイルの大きさと更新時刻に、Unix では device・inode・状態変更時刻（ctime）、それ以外ではパスと作成時刻を加えたものです（`cp -p` や Nix のように更新時刻をそろえても、別のファイルや書き直したファイルを区別します）。
パックと manifest は、すべてのソースの構文解析が成功したときだけ、内容が変わった場合に書き換えます。読めない root、symlink、大きさの超過、形式・compiler・プロジェクトの不一致、checksum の不一致、復号の失敗はすべて miss として構文解析し直し、診断を出しません。構文解析の順序、`Diagnostics::is_full` による打ち切り、エラーの連結は cache なしと同じなので、IR、診断、警告、終了コードは cache の有無や cold／warm で byte 単位で同じです。書き込みは一時名からの rename で、書いたプロセスが合計 1 GiB、更新から 30 日、1 時間より古い一時ファイルの規則で掃除します（1 回に 4,096 件を調べ、最大 128 件）。`BuildCache::evict` は `frontend` という名前を扱いません。言語サーバー（`analyze_inputs_semantic`）は cache を使いません。`TSUZURI_CACHE_DIR=`（空）で無効になり、build・run・script の `--no-cache` は両方の cache を、repl の `--no-cache` は成果物の cache を止めます（repl は frontend cache を使いません）。検証は `src/syntax_codec.rs`・`src/frontend_cache.rs` の単体テストと `tests/frontend_cache.mjs` です。

### 並列フロントエンド（PB04）

`src/parallel.rs` の `ordered_for_each` は、項目 `0..count` の関数を scoped thread（名前 `tsuzuri-frontend`、スタック 8 MiB）で評価し、結果を呼び出しスレッドの consumer へ項目の順に 1 つずつ渡します。consumer は並列化前のループそのもので、`Diagnostics::is_full` による打ち切り、先着優先の重複除去、警告の順序がスレッド数に依存しません。ワーカーが読むのは共有の不変データだけで、書くのは項目ごとの結果だけです。1 スレッドまたは仕事が少ない段は呼び出しスレッドだけで動き、consumer が読んだ項目だけを評価するので、並列化前と同じ経路です（読まれなかった項目のワーカーの panic は無視し、読まれた項目の panic は元の payload のまま呼び出しスレッドへ再送出します）。上限は `Parallelism`（`TSUZURI_THREADS`。CLI は未設定のとき `min(core 数, 8)`、ライブラリは 1、1 から 32 以外は `E2000`）で、スレッド数は `BuildOptions` やキャッシュのキーに入れません。`Send`／`Sync` が要る型は `check.rs` の `thread_safety` で表明します（`recursive::Cache` は `Mutex<BTreeMap>`、`Interner` は `Arc<str>`）。並列にする段は次の 4 つです。

- 構文解析（`analyze_inputs_with` の `parse_input`）: `FrontendCache::lookup(&self)` がキーの SHA-256・復号・構文解析・interface hash を行い、入力の順に読んだ分だけを `commit(&mut self)` が stats・entries・pack からの除去として記録します。同じ内容が先に復号できなかった入力は miss として数えるので、pack・manifest・stats は逐次の `parse` と同じです。ソースの大きい順に 1 つずつ取得し、合計 64 KiB 以上かつ 4 個以上で 2 ワーカー以上、32 KiB ごとに 1 ワーカーです。
- 関数の本体の型検査（`check_modules_collect` の本体の loop）: `&mut (String, FunctionDecl)` を項目として渡し、`CheckedFunction`・`Checker`・復元した診断を宣言の順に取り込み、`Diagnostics::is_full` で打ち切ります。signature の推論、`solve_members`、`$entry` は逐次です。
- 関数ごとの型検査の仕上げ（同じ関数の finish の loop）: `Checker` と `&mut CheckedFunction` を項目として渡し、`name_uses`・警告・診断を関数の順に取り込みます。
- 所有権の検査（`ownership::check_functions`。`infer_copy_all` と具体化後の検査）。

後ろの 3 つは関数 64 個ごとに 1 ワーカー（2 ワーカーには 128 個）で、8 個ずつ取得します。標準ライブラリの宣言だけで hello も数百個あるので、hello の `check` も worker を使います（`FUNCTIONS_PER_WORKER` は hello の計測で 256 から 64 に下げました。[docs/benchmarks.md](benchmarks.md)）。並列にしない段は、プロジェクトの読み込み、名前と signature の収集、`solve_members`、`constants::fold`、`recursion::check`、`warnings::unused_private`、`polymorph::specialize`（worklist と上限の判定が順序に依存します）、`closures::lower`、`llvm::emit`（文字列定数・metadata・trap 点の番号が出力順で決まります）、キャッシュの書き込みです。

検証は `src/parallel.rs`・`src/lib.rs`・`src/frontend_cache.rs` の単体テスト（thread 数 1・2・3・8 の診断と型付き構造の一致、上限の打ち切り、example と fixture の全 project）と、実際の CLI で出力・キャッシュの bytes・不正な値を照合する `tests/parallel_frontend.mjs` です。

### 高速デバッグバックエンド（PB05）

`--backend fast`（`-O0` の native `build`・`run`・`test`）は、`src/llvm*.rs` が出す `-O0` の LLVM IR のテキストをそのまま入力にして、`src/backend/`（cargo feature `fast-backend`。`cranelift-codegen`・`cranelift-frontend`・`cranelift-module`・`cranelift-object` と `target-lexicon` に依存し、LLVM の C API には結合しません）で native の object に翻訳し、リンクだけを Clang に任せます。IR は `module.ll` に書かず、メモリ上で渡します。`--backend` を付けないビルドは、IR・object・実行ファイルとも変わりません。

- 読み取り（`lex.rs`・`module.rs`・`func.rs`・`parse.rs`・`types.rs`）: module を 1 回走査して、型・global・`declare`・`define` の header と本体の範囲を集め、外から見える define と、アドレスが取られる関数・global の初期化子から参照される関数を root にして、届く関数と global だけを宣言します（`-O0` の Clang が出す未使用の `internal` 関数は翻訳しません）。本体は worker が 1 関数ずつ解析し、命令 1 つを 1 つの記録にします。知らない命令・intrinsic・属性・型は `E2002`（構成要素と関数の名前つき）で、別の意味のコードにはしません。外から見える関数（`declare` と、external・weak の `define`）の `byval`・`sret` の引数とその呼び出し、ポインタ越しの `byval`・`sret` の呼び出しがそうです。`musttail` の呼び出しは、1 つの call と、その結果を返す `ret` に分けて読み（`Inst::Call.must_tail`）、`return_call` にできる組は 1 つの命令にし、できない組は通常の call にします（下の「保証された末尾呼び出し」）。内部関数（runtime の `numeric` の `fastcc` の関数）の `byval`・`sret` は、呼び出す側が複写した領域へのポインタと結果の領域のポインタとして、そのまま翻訳します。`alloca` は入口の block だけ、1 行 1 命令です。16 byte を超える alignment の `alloca`（256 bit の vector）は、slot を `alignment - 16` byte 余分に取り、アドレスを alignment に切り上げます（Cranelift の stack は 16 byte にしか揃わないためです）。
- 翻訳（`lower.rs`・`ops.rs`・`mem.rs`・`call.rs`・`intrinsics.rs`・`abi.rs`・`data.rs`・`target.rs`）: 整数は最小の container（`i8`〜`i128`）に置き、中間の幅は演算のたびに折り返します。16 個以下の leaf の aggregate と vector は値の組、それより大きい aggregate は stack の slot への参照で、大きい phi は辺ごとに slot へ複製します。内部関数の ABI は leaf に分解した引数で、外部の関数は scalar だけです。Cranelift に無い `fcmp` の述語・`fshl`・`*.sat`・`ctlz` などは基本の操作の組で作り、AArch64 の atomic（8・16 bit の `umin`/`umax`、32 bit の compare-and-swap）は Cranelift が前提にする上位ビットを先に消します。
- 並列と決定性: `src/parallel.rs` の `ordered_for_each` で、関数の解析・lowering・Cranelift の compile を並列にし、本体を IR の定義の順に object へ渡すので、object は thread 数に依存しません。ISA は `isa::lookup` の固定の設定で、host の CPU を検出せず（x86-64 は baseline）、`opt_level=none`・PIC・frame pointer あり・inline の stack probe です。Mach-O は build version を付け、ELF は関数と data ごとに section を分けます。`unreachable` と `llvm.trap` は AArch64 で `brk`、x86-64 で `ud2` になり、LLVM と同じ signal で終わります。
- 検証: `tests/fast_backend.rs`（layout が Clang と一致、E2002 の表、IR の翻訳、決定性、`-g` の行表、`test` の runner）、`tests/fast_backend_ops.rs`（native の約 940 の演算と wasm32 の約 970 の演算を、Clang の object と翻訳した object のそれぞれで C の host か Node から呼び、bit 単位で比べます）、`tests/fast_backend.mjs`（option の表、hello・trap・`--trap-info`・`run`、thread 数の決定性、cache、wasm32 の object と Clang の `__int128` の object のリンク、`test`、`-g`。`programs` は examples と fixtures を両方のバックエンドで動かして比べ、`debug` は `-g` の行表を `llvm-dwarfdump` で検証して `llvm` のものと比べ、`parity SUITE...` は既存の suite を 2 つの wrapper の下で動かします）。
- wasm32（`wasm/`。D13）: `--backend fast --target wasm32 [--emit wasm|object]` は、同じ読み取り部（pointer の幅だけ 4 byte）で IR を読み、Cranelift を通さずに wasm の命令を直接書きます。再配置可能な object（type・import・function・elem・data count・code・data・`linking`・`reloc.CODE`・`reloc.DATA`・`target_features`）で、memory・`__stack_pointer`・`__indirect_function_table` は Clang の object と同じく `env` から import します。リンクは今までどおり `wasm-ld` で、`numeric`・`math` の object（PB01）はそのまま使います。値は local に置き（最後の使用のあとは別の値が同じ local を使います。phi・frame pointer・戻り値は専用）、定数・frame と global のアドレス・`extractvalue` などの組み替えは local を使いません。block は 1 つの `loop` の中の `br_table` の dispatch で、前向きの分岐は、後ろの block の始まりである開いた `block` の終わりへの `br` で行い（dispatch を通りません）、後ろ向きの分岐だけが次の block の番号を `$pc` に置いて dispatch に戻ります。phi は辺で一斉に代入します（大きい aggregate の phi は先に複写します）。alloca は固定の frame で、`__stack_pointer` を入口で下げ、出口の block で戻します（16 byte より強い alignment は入口で切り下げます）。`i128` は 2 つの `i64`（定数の shift・加減算・比較は語の演算、乗除と変数の shift は `wasm.ll` の `__multi3` などの呼び出し）、atomic は threads を使わないので普通の load と store、`llvm.*` は `memory.copy`・`memory.fill`・`trunc_sat`・`i64.clz` などの命令です。128 bit の結果の返し方は、リンクする runtime の object（Clang が作ったもの）が import する `__ashlti3` などの型で決めます（2 つの結果か、先頭の pointer か）。決める object がない `--emit object` などでは pointer です。Clang 21 は `-mmultivalue` を付けても pointer で返し、object が定義する弱い `__multi3` などの型が、Clang の object の呼び出しの型と食い違うと `wasm-ld` が warning を出して実行時に失敗するからです（`tests/fast_backend.mjs` が Clang の `__int128` の object とリンクして確かめます）。`E2002` になるのは、`llvm.fma`・128 bit の積の overflow・表にない `llvm.*`（`llvm.round`・`llvm.minnum`・`llvm.maxnum` など）、1〜16・32・64・128 bit 以外の幅の整数の load と store、可変引数の呼び出し、基本ブロックが 65,521 個を超える関数（`br_table` の対象が V8 の上限 65,520 を超えます）、local が 40,000 個を超える関数（値の数ではなく、最後の使用のあとに使い回した local の数）、runtime の object が 2 つの結果で返すと決めたときの pointer 越しの 128 bit の結果の呼び出し、`byval`・`sret` です。`musttail` は通常の呼び出しとして翻訳します（wasm32 の `--backend fast` は `--wasm-feature tail-call` を受け付けないので、既定の IR に `musttail` は現れません）。
- デバッグ情報（`debug.rs`、`-g`）: IR の `!DILocation`・`!DISubprogram`・`!DIFile` を `!N = ` の行から読み（`Metadata`）、命令の `!dbg !N` を Cranelift の `SourceLoc` として lowering が付けます。compile のあと、`MachBuffer` が返す「同じ位置から出た機械語の範囲」を関数ごとの行表の列にし、`gimli::write` が 1 つのコンパイル単位（`DW_TAG_compile_unit` と、関数ごとの `DW_TAG_subprogram`・行の sequence・`DW_AT_ranges`）にします。アドレスは関数の symbol への relocation で、Mach-O は `dsymutil` がオブジェクトのアドレスと見なす値を section 相対で置き、ELF は関数の symbol と section の symbol を使います。section は `__debug_*`（Mach-O）か `.debug_*` で、DWARF 4 です。関数の名前・ファイル・行・列だけで、変数と型は書きません。

### watch（PB06）

`tsuzuri watch check|build|test` は、`cli`（`src/main.rs`）を同じプロセスで繰り返し呼びます。`cli` は診断と警告を書き込み先（`err`）へ書いて終了コードを数値で返すので、出力は単独のコマンドと byte 単位で同じで、回ごとの状態行だけが `watch` のものです。`src/watch.rs` の `Watcher` は、`Project::load` が読んだユーザーのソースとマニフェストを、本文と `FileStamp`（`lstat` の大きさ・更新時刻、Unix では inode と状態変更時刻）で覚えます。スナップショットは実行の前に取るので、実行中の保存は次の周期で見つかってもう一度実行されます。100 ms ごとに `FileStamp` を比べ、変わったファイルだけを読み直して本文が違うときだけ変更とします（`touch` と、内容を変えない保存は実行しません）。1 s ごとに、`Project::load` と同じ走査（`collect_sources`。隠しファイルと上限が同じ）でファイルの追加と削除を調べます。ファイルを読む前に stamp を取るので、読んでいる間の書き込みは違う stamp として見えます。読んだ時刻が stamp の時刻から 2 s 以内のファイルは、更新時刻の粒度のために同じ stamp の書き込みを見逃しうるので、その時間が過ぎたあとにもう一度だけ読みます（git の racily clean と同じ考え方です）。Unix 以外は状態変更時刻がなく、プログラムが更新時刻を戻せるので、1 s ごとに全ファイルを読み直します。検証は `src/watch.rs` の単体テストと `tests/watch.mjs` です。

### 常駐状態（PB06）

`src/resident.rs` の `ResidentState` は、`watch`（と `serve`）のプロセスが実行のあいだ持つものです。`read_source_text`（`src/driver.rs`）が読んだソースの本文と、`cache::file_digest` が計算した SHA-256（`build_key` はコンパイラ自身の実行ファイル、約 9 MB を毎回ハッシュします）を、読む前に取った `FileStamp` といっしょに覚え、stamp が同じなら、ファイルを開かずに（`lstat` だけで）返します。覚えたものを使えるのは、読み始めた時刻が stamp の時刻から 2 s 以上あとのときだけです（`watch` と同じ racily clean の規則）。読みに失敗したものは覚えないので、エラーは常に実際に読んだ結果です。`Scope::enter(&mut state)` が状態をコマンドを実行するスレッドの thread-local へ入れ、`Drop` で戻します（入れ子と panic に対して安全）。`Project::load*` と `build_complete` の引数は変わりません。`Scope` がないとき `resident::clock()` は `None` で、2 つの呼び出し先は今までの処理だけを行うので、1 回だけ走るコマンドの動作は変わりません。Unix 以外は stamp がすべての書き込みを示さないので、何も覚えません。覚えたものの合計が 1 GiB を超えると、`trim()` がすべて捨てます。状態は、起動したコンパイラの実行ファイルの識別（`frontend_cache` の G17 の識別と同じ式）も固定し、`Scope` の中の `frontend_cache::compiler_identity()` はそれを返します。`build_key` に入るコンパイラ自身のハッシュも、固定した識別と同じファイルを読んだ最初の 1 回（読む前と後で識別が同じことを確かめます）で決めて持ち続けます（`resident::compiler_digest`）。コンパイラのファイルが差し替えられても、プロセスは自分の（古い）識別と古いハッシュでキャッシュを書き、差し替えのあとの最初の鍵なら、ハッシュを作らずキャッシュなしで進みます。`watch` は、待っているあいだ（`Watcher::wait_for_change` が 100 ms ごと）と回の始めに `compiler_changed()` を見て、真ならその回を始めずに止まります。`reset()` は、`serve` が panic のあとで読んだものを捨てるためのもので、固定したコンパイラは残します。`TSUZURI_TIME_PASSES=1` の報告は、`watch` では 1 回ごとに 1 行です（`Report::start()` が、前の回の段を捨てます）。`lsp` と `repl` も同じ `Scope` を、プロセスの間ずっと使います（採用の条件は、REPL の cache hit の再評価で 5% 以上、または LSP の 1,000 module の変更の応答で 100 ms 以上の短縮で、Apple M1 Max の macOS では REPL が `-O0` で約 7〜9%（約 15〜20 ms）、`-O3` で約 30%、LSP が 1,000 module の最小で約 455〜480 ms でした。1,000 module の LSP は、ファイルを開く費用が大きいこの機械の性質が大きく、環境によって差が変わります）。LSP は `lsp::serve` が、REPL は `repl::run` が、`ResidentState` の `Scope` を入れて使います。測定（200 module の `check`、1 module の本文を編集、Apple M1 Max の macOS）では、`watch` の `load` が 17〜26 ms から 2.2 ms になり、1 回の段の total は 292〜297 ms から 264〜267 ms でした（`serve` は [ベンチマーク](benchmarks.md)の PB06 の節に、条件と生の値があります）。

### 常駐ビルドサーバー（PB06）

`tsuzuri serve DIR`（`src/server.rs`、Unix だけ）は、プロジェクトのルートごとに 1 つのプロセスで、`check` と `build` の要求を実行します。構成は、(1) accept スレッド: 接続を受けると hello（`protocol`・`server`・`version`・`pid`・`root`）を書き（待っているクライアントが hello の待ち時間 2 s で諦めないように、先に書きます）、要求の行を読み（1 MiB まで、2 s 以内）、実行が要らないものはその場で答えます。`status` は共有の数（`Shared`）と、最後に終わった要求のあとに `Handler::status` が言った値から作り、`shutdown` は旗を立てて実行中の要求の番号と秒を返し、protocol・too-large は断ります。実行する要求は `Job` にして、待っているものが 16 件未満なら channel に積み（満杯なら `busy` の応答を書いて閉じます）、`shutdown` の合図も同じ channel に積みます。このスレッドの行（断り・`status`・`shutdown`）は、(3) のログスレッドに渡します。(2) `serve` を呼んだスレッド（`main` なので stack は 8 MiB）が、channel から取り出して 1 つずつ実行します。取り出した `Job` は、クライアントが自分の側を閉じていれば（待っているあいだに Ctrl-C で止められた）、非ブロッキングの 1 byte の `read` が EOF になるので、実行せずに `abandoned` に数えます（クライアントは返事を読み終わるまで閉じません）。実行中は `running` に番号と開始時刻を置き、`shutdown` の旗は実行が終わってから見ます（実行中の要求は止めず、待っている要求は実行しません）。実行が終わったら、`Handler::status` の値・`served`・`running` を更新してから応答を書きます（応答を読んだクライアントの次の `status` が古くならないように）。ここが `cli`（`src/main.rs`）を `Scope`（`ResidentState`）の中で、標準出力・標準エラーの代わりの `Vec<u8>` に書かせて呼びます。(3) ログスレッド（`tsuzuri-serve-log`）: サーバーのログ（標準エラーの `serve: ...` の行）を書くのは、このスレッドだけです。ほかのスレッドは、行を `Log::line` で長さ 256 の `sync_channel` に `try_send` し、入らなければ捨てて数えます（`LogCounts`: 次に積めた行が運ぶ `missed` と、`status` の `log_dropped` になる合計）。標準エラーは、読む人のいないパイプや止められた端末で、いつまでも書けないことがあります（Linux の CI で、読まれない標準エラーのパイプが数百行で満杯になり、要求を実行するスレッドが `write(2)` で止まって、クライアントが返事を待ち続けました）。だから要求・`status`・`--stop` は、ログの書き込みを待ちません。行は 4096 byte に切り、改行は `\n` と書くので（クライアントが送る cwd や名前が行を偽造できません）、1 行が `write_all` 1 回（と `flush`）で書かれ、捨てた行の数は、捨てた場所の次の行の前に `serve: N log lines dropped` の行として書かれます。Handler が書く `log` は `HandlerLog` が行に組み立て、同じ `Log` に渡します。スレッドは join しません（止まった書き込みがプロセスの終了を妨げないように）。終わるときは、ソケットとロックを消してから、`Close` を積んで 1 s まで待ちます。コマンドがパニックしたときの標準のメッセージ（`panic` hook）だけは、パニックしたスレッドが標準エラーに直接書きます。`parallel.rs` の worker は段ごとの scope で、常駐しません。コマンドを実行する側（`CommandServer`）は `Handler` として `serve` に渡され、`serve` は `cli` を知りません。通信は 1 行 1 message の JSON（protocol 1）で、1 接続 1 要求、要求は 1 MiB（読み始めから 2 s で届く）、応答は 16 MiB までです。要求を処理する順番は、(a) protocol（JSON、`protocol`、`op`）、(b) コンパイラのファイルの自己点検（起動時の識別と `current_exe` の今の stat。違えば `compiler-changed` で断って終了）、(c) `compiler` のパス、(d) `cwd`、(e) 環境（`is_relevant_variable` が選ぶ変数だけ）、(f) `Handler::run`（引数の解釈、`check`・`build` だけ、`project_root` が同じ。違えば `unsupported`・`root`）です。`catch_unwind` で panic を `internal` で断り、`ResidentState::reset` で読んだものを捨てます（固定したコンパイラは残すので、そのあいだにコンパイラが差し替えられていれば、次の要求が `compiler-changed` になります）。出力が cold と byte 単位で同じになるように、サーバーが出力する唯一の経路は応答です。コマンドの標準エラーは `resident::capture` の `Captured` に書かれ、書き込み先を持たない `TemporaryDirectory` の `Drop` の警告（`warning: cannot remove ...`）も `resident::warn` で同じ buffer へ順序を保って入ります（`Scope` の外ではこれまでどおり標準エラーです）。`TSUZURI_TIME_PASSES` は `timings::begin_command`・`take_report` で要求ごとの 1 行にして応答の標準エラーの最後へ置き、クライアントは `timings::silence_report` で自分の報告を出しません（`main` の `Report` がその合図を見ます）。環境の比較の集合は `server::is_relevant_variable`（`cache::KEY_ENVIRONMENT`、`TSUZURI_*`、キャッシュ・一時ディレクトリ・ロケール・SDK の変数）で、`src/` の `env::var`／`var_os` が読む名前がすべてこの集合に入ることを `server` の単体テスト（`every_environment_variable_that_the_compiler_reads_is_compared`）が `src/` を走査して確かめます（名前が文字列でない読みと、環境全体を読む箇所は、数と場所を固定して、増えたら見直させます）。クライアント（`server::request`。`check` と `build` が引数の検査のあとに呼ぶ）は、安い確認から順に行います: ソケットのディレクトリが私用か（`lstat` 1 回）、ルートからソケット名を作る、ソケットが socket か。どれかが偽なら、接続せずに自分で実行します。接続後も、hello・応答の検査に失敗すれば、何も書かずに自分で実行します（`check` と `build` のクライアントは応答が来るまで時間制限なしで待ち、`status` と `--stop` は 10 s で諦めます）。`resident::is_active()` のとき（`watch`・`serve` の中）は尋ねません。信頼の境界は、`0700` のディレクトリ（作成時、または既存のものの種類・権限・所有者の検査。所有者は、そのディレクトリに作った一時ファイルの `uid` と比べます）だけで、相手の資格は確かめません。ソケットは `0600`、名前はルートの SHA-256 の先頭 16 桁です。起動時はまず、ソケットの隣の `<名前>.lock` に `File::try_lock`（`flock`）で OS のロックを取り、終わるまで持ちます（取れなければ二重起動の `E2001`。プロセスが終われば、強制終了でもロックは外れます。ロックのないファイルシステムでは、ロックなしで続け、ログに書きます）。同時に始めた 2 つのサーバーが、同じ古いソケットを消して `bind` し、片方のソケットに誰も届かなくなることを防ぐためです。そのうえで `bind` が `AddrInUse` なら、socket に接続してみて、答えれば二重起動（`E2001`）、拒まれれば古いソケットとして消します。終わるときは、自分が作ったソケット（`dev` と `ino` が同じもの）を消してから、ロックを外します。macOS の注意が 2 つあります: 相手がすでに閉じたソケットへの `shutdown(Write)` は `ENOTCONN` で失敗するので、クライアントは半閉じをしません（要求は改行で終わり、サーバーは改行までを読み、クライアントは応答を読み終わるまで閉じません）。相手が閉じたソケットへの `set_read_timeout` は `EINVAL` なので、サーバーは読みを続けます。Unix 以外では、`serve` と `serve --status`・`--stop` が `E2000`、`request` が常に `None` で、`--no-server` は受け付けて何もしません。検証は `src/server.rs` の単体テスト（protocol、環境の drift、プロセスの標準出力・標準エラーへ直接書く箇所の drift（`src/main.rs` は、直接書く関数の一覧を `src/main.rs` の単体テストが固定する）、ディレクトリの権限、偽の handler による主ループ: 断り、busy、idle、古い socket、二重起動と同時の起動、実行中の `status` と `--stop`、クライアントが去った待ち要求、止まった書き込み先に対するログ（捨てる・数える・1 行 1 回の書き込み・終了が待たない）、`status` の応答の時間切れ、偽のサーバーへのクライアントの fallback）と、実際のプロセスで状態ごとの出力・成果物を `--no-server` と照合する（Clang を止めて、実行中の `status`・`--stop` と、去ったクライアントの要求を確かめる場面と、読まれない標準エラーの pipe に何千もの断りと 1 つの `check` と `--stop` を通す場面を含む）`tests/server.mjs` です。

### 言語版（G19）

`Edition(u16)`（`src/syntax.rs`。`E2026`、`E2027`、`LATEST`、`MANIFEST_DEFAULT`、`SUPPORTED`）は、予約語と引数規約の版です。`Manifest::edition`（`[package]` の `edition`。省略は固定の `MANIFEST_DEFAULT = 2026`、対応しない値は `E0002`）から `driver` が `SourceFile::edition` を決めます。manifest のない CLI 入力、`script`、`repl`、std と新規パッケージは `LATEST = 2027`、依存は自分の manifest の値です。`SourceInput`・`ModuleInput` は版を lexer・parser・checker に渡します。版を受け取らない Rust の `analyze*` と単独 `check::check` は 2026 の互換入口で、`analyze_with_edition`・`analyze_modules_with_edition`・`analyze_inputs_all` は明示版を使えます。lexer の `EDITION_KEYWORDS`（現在は空）とモジュール名の予約語検査もソースの版を使います。2027 は署名収集時に読み取り・可変規約を物理的な参照型へ展開し、所有権検査と LLVM はその解決済みの型を使います。frontend cache と build cache の鍵は edition を含みます。`tsuzuri fmt` は入力のディレクトリ（ファイルなら親）の manifest を読み、なければ `LATEST` です。

`edition_migration::prepare` は旧版の semantic index から署名と標準 API の参照を集め、値渡しを `own` と `_owned` 入口で保存します。同名の利用者定義、依存ソース、既に移行した綴りは書き換えません。対象版で全体を再検査し、通常ファイルと内容の一致を確認してから atomic に置換し、manifest は最後に更新します。失敗時は既に置換した内容を戻します。単一ファイルの指定では package の版を変えません。std の開発用ツリーは埋め込みソースとの一致を条件に検査し、std だけの semantic index は未使用関数の本体も含めます。

### 非推奨と W1005（G19）

`Documentation::deprecation` が説明の `@deprecated` の行を読みます。`warnings::deprecations` は、std の、または依存パッケージ（root パッケージでないもの。root に依存するパッケージはありません）の公開の `def`・`record`・`union` の印を集め、空でなければ `check_modules_collect` が `semantic::collect` を 1 回だけ走らせます（`SemanticIndex` を求められたときとの共有で、使える印のない project は追加の解析をしません）。`warnings::deprecated_uses` は、利用者のモジュールにある `SemanticIndex::references` のうち、関数、record、union、union の case（印は union のもの。`Union.Case` は case の位置に 1 件）の定義が別のパッケージか std の印を持つものに `W1005` を出します（同じ span は 1 回）。同じパッケージかどうかは `ModuleInput::package` で決まります。これは依存パッケージの `PackageId` で、root パッケージ、manifest のないソース、std は `None` です（`Project::dependency_package`）。警告は、ほかの警告と同じ経路（`CheckedModule::warnings`）で CLI・`--deny-warnings`・LSP・frontend cache の有無に関係なく届き、生成コードには作用しません。

### 知識グラフ（G26、D-60）

`tsuzuri::kg::collect`（`src/kg/mod.rs`）は、プロジェクトを 1 回だけ解析して知識グラフ（`KgModel`）を作ります。解析は `analyze_inputs_captured` で、`check_modules_collect` に `Some(&mut Capture)` を渡します。フックは 2 か所で、どちらも checker の状態を**読むだけ**です。H1: 意味の索引（`semantic::collect`）を capture があるときにも作り、`@deprecated` の警告（`W1005`）は宣言があるときだけなので、警告の集合は変わりません。H2: `ownership::infer_copy_all` の直後、`polymorph::specialize` がモジュールを消費する前に `kg_capture::collect_abstract` を呼びます。capture を渡さない解析は capture の仕事を 1 つもせず、生成 IR・診断・警告は capture の有無で変わりません（`analysis_with_capture_keeps_the_ir_identical`）。

事実は 2 つの視点から作ります。**宣言の視点**（`Capture`。単相化前）は、ジェネリック関数も 1 つの宣言として扱い、ノード（`Program` のすべての field を `..` なしで分解するので、宣言の種類を足すと compile error になる）、辺（`calls` は抽象本体の `Call` と値としての関数、`uses_type` は意味の索引の参照、`tested_by` と `recursion` は `recursion::check` の参照グラフの有界の幅優先と強連結成分）、ループ、所有権の出来事（`ownership::events_abstract`。`check_body` の event の sink で、sink を渡さない検査は何も変わらない。`ownership_sink_does_not_change_diagnostics`）を集めます。**実体の視点**（最終の `CheckedModule`。単相化と `closures::lower` の後）は、具体型の要る効果・トラップと確保の箇所・`copies::sites`・特殊化の数・レイアウトを作り、各関数を宣言へ帰属させます（持ち上げたラムダは親、`.$mono.` は基底の名前、`$builtin` の包みと std は対象外）。`closures::lower` は組み込み関数への参照を `$builtin` の関数で包むので、トラップの判定は包みの本体を見ます。効果の証拠の鎖は、効果を直接持つ関数への最短の鎖です（幅優先の距離で決まり、相互再帰でも有限）。各関数は鎖の次の関数への 1 本のリンクと鎖の長さだけを持ち（長い呼び出しの鎖でも記憶は関数の数に比例）、行を書くときに 100 件までたどります。`user_drop` は、値の型か、それが参照を通らずに格納する型に利用者の `Drop` があるときです（純粋性の検査と同じ走査）。レイアウトは `llvm::layout` の `layout_bound`・`record_slots`・`union_layout` を `target_scope(false)` の中で呼びます（IR は変わらない）。

出力は決定的です。行は ID と位置で整列し、`HashMap` の反復順を使いません。ID は位置に依存せず、衝突はソースの位置の順に `#n` を付けます。グラフのアルゴリズムは `src/kg/algo.rs` の 1 か所にあり、照会・`impact`・図も同じものを使います（D-60）。

### 知識グラフのファイル（G27、D-60）

`check`・`build`・`run`・`test`（`src/main.rs` の `cli`）は、解析の**前**に `kg::build::plan` で、書くか（設定とプロジェクトかどうか）、置き場が知識グラフか（マーカー `.tsuzuri-kg`、ソースを含まないか）、新しいか（入力の SHA-256・schema・`manifest.json` の `compiler`・マーカーの stamp）を決めます。新しければ解析は capture なしの `analyze_cached` で、知識グラフの仕事は manifest の読み取りとハッシュだけです。書くときは `analyze_cached_captured` が解析を 1 回だけ行い（`Project::analyze_cached_core` に capture を渡す）、診断と警告を書いた後、`--deny-warnings` の判定の前に `kg::build::complete` が書きます。失敗は `W2001` の警告 1 行で、終了コードと診断は変えません。stamp は `frontend_cache::compiler_identity` と知識グラフの版のハッシュで、機械ごとの値なのでマーカーにだけ置き、`manifest.json` には入れません（名前が `.` で始まらないファイルは、どの機械でも同じ byte）。

`write::publish` は、置き場の親の `TemporaryDirectory`（`.tsuzuri-<pid>-<n>`）の `next/` にファイルを並列に書き、マーカーを最後に書き、マーカーのある既存の置き場を `previous-<k>` へ rename してから `next` を rename します（最大 3 回。別のプロセスが先に置いたら後勝ち）。マーカーのないディレクトリは決して上書きしません。`collect_sources` はマーカーのあるディレクトリを読みません。読み手（`store::with_snapshot`）は `manifest.json` に載ったファイルだけを、途中のディレクトリがリンクでないことを確かめて読み、各ファイルの SHA-256 を manifest と比べます。一致しないとき、manifest が変わっていれば書き手の入れ替えをまたいだので初めから読み直し（3 回まで）、変わっていなければ壊れています（`kg_broken`）。`Manifest::parse` は `[A-Za-z0-9_.%~-]` の名前を `/` でつないだパスだけを受け付けます。`kg verify` と `kg export` は同じ 1 回の公開のバイト列（`read_snapshot`）を使います。新しさは、`inspect` の後に別の書き手が置き換えうるので、読んだ公開の manifest とマーカーの stamp（マーカーは書き出しごとに違う印を持ち、manifest の前とファイルの後に読んだ全文が違えば、同じコンパイラの書き出しでも読み直す。`store::Snapshot`）について `manifest::judge`（schema・コンパイラと stamp・入力の SHA-256・完全か）で確かめます（`KgData::open_fresh`、`kg verify`、`kg export`）。`kg clean` は `build::protect` を通し、マーカーがあってもプロジェクトのソースを含むディレクトリは消しません。ファイル名の stem は 200 バイトまでで、長い ID は先頭と SHA-256 の 16 桁です。読み手の `KgData` は、読み込みのときに一度だけ `KgIndex`（全端点を ID の byte 順に番号付けし、関係ごとの前向き・後ろ向きの辺の表）を作り、カード・照会・図はこの索引だけを読みます。カードは `brief::Card` の優先度と `spread` で上限に収め、文字数は `chars().count()` で数えます。std の知識グラフ（`std_kg`）は std のソースだけを `include_std` の capture で解析し、キャッシュの `kg-std/<stamp の先頭 16 桁>/` に置きます（新しい 3 世代を残す）。増分（モジュールごとの事実の書き出しの再利用）は、測った節約が知識グラフの生成時間の 10% に届かないため入れていません（`docs/benchmarks.md`）。

### 知識グラフの照会と変更の影響（G28、D-60）

照会の関数（`kg::query`）は `&KgView`（プロジェクトの `KgData` と、必要になったときだけ作る std の `KgData`）を取り、ディスクに触れません。`KgData` は `facts/`・`layout/` のモジュールごとのファイルを開くときには読まず、`Tables` が、行を求められたモジュールのファイルだけを初めて使うときに読みます（`OnceLock`。ファイル名はモジュールの key の stem で、大文字小文字だけが違う stem の `~n` も同じ組として読む。組は `Tables::new` で一度だけ作り、読んだファイルの行は ID で引けるようにするので、`fact` はモジュールの数にも行の数にも比例しません）。遅れて読むファイルも、開いたときの `manifest.json` の SHA-256 と比べます。違えば（書き手が入れ替えたか壊れている）その行は使わず、最初の失敗を `KgData::intact` が返し、コマンドは答えを出す前にそれを確かめます（`kg_stale` の「replaced while it was read」か `kg_broken`）。`impact` は 2 つの `manifest.json` で SHA-256 が同じ facts のファイルを読みません。base の知識グラフはそれ自身の `target/tz/kg` に置き（`--kg-dir`・`TSUZURI_KG_DIR` は作業ツリーのもの）、head の置き場が base と同じなら、head を作り直す前に base の表をすべて読みます（`KgData::read_all`）。`--no-refresh` の std は `StdSource::Cached`（キャッシュにある完全な std の知識グラフだけを読み、作らない）です。ID は決定表の段（完全一致、`::`・`.` の後ろの一致、大文字小文字を除いた版、前方一致）で解決し、曖昧なら候補を、未知なら最後の名前の OSA の編集距離が近い ID を返します（`kg_ambiguous_id`・`kg_unknown_id`）。`callers`・`callees` は `KgIndex` の `calls` の表の上の `algo::bfs`（`callees` と `path` は始点を宣言する知識グラフ、`callers` はプロジェクトのものから）（層ごとに整列した幅優先。各 ID は最も浅い深さで 1 回）、`path` は選んだ関係の辺の和の上の `algo::shortest_path` です。テキストは `brief::Card` の優先度で `--budget` に収めます。`kg find`（`kg::find`）は、ノードの `params`・`result` の型の文字列を小さな型の構文木に読み（field と case は、持ち主の record・union の署名の型引数を同じ型変数で共有します）、問い合わせとエントリの両方の型変数を 1 つの代入で単一化して、G25 の段（`exact`・`borrow`・`partial`）に分けます。固定長の配列の長さの変数も同じ代入で決めます。エントリの型の名前は `Types::resolve`（そのモジュールの型、ノードとその持ち主の `uses_type` の辺の先、同じ名前空間の型）で型のノードの ID にし、名前どうしは一方が他方の `.`・`::` の後の末尾のときに一致とします（`same_name`）。型変数が基本の型に決まった制約は、checker と共有する `check::builtin_instance_by_type`（`Classes::intrinsic` の、型だけで決まる規則）と instance のノードで確かめ、ほかの制約は確かめません。

`kg::impact::diff(base, head, …)` は 2 つの `KgData` だけを読みます。ノードを ID で対応させて `api-diff` と同じ文言の変更を作り、事実（効果・箇所の数・再帰）と警告の多重集合を比べます。届くテストは、変わったモジュール（`manifest.json` の `modules[].source` の差）のノードから、`calls` の逆向きの辺と、class のメソッドの呼び出しを各 instance のメソッドへ延ばす仮想の辺の上で、`algo::reaches_beyond`（深さ 8 の先にまだ呼び出し元があるか）と親つきの `algo::bfs` を回して集め、テストごとの最短の鎖を `--json` の `paths` に出します。`--git REV` は `kg::git::Checkout` が、リポジトリの外の一時ディレクトリに `core.hooksPath` を空のディレクトリにした `git worktree add --detach` で取り出し（取り出す先のディレクトリはリポジトリと同じ名前にします。マニフェストのないプロジェクトは名前空間をディレクトリの名前から取るため）、`Drop` で `worktree remove --force` します（`target/` の下に置くと `collect_sources` が読むため）。`tsuzuri test --affected`（`src/main.rs` の `select_affected`）は、その結果のテストの ID を `tests.jsonl` の番号に引いて `TestOptions.indices` に入れます。`BASE` が知識グラフのディレクトリなら、`cli` が解析と知識グラフの書き出しの前に全部読みます（`base_graph`。プロジェクト自身の `target/tz/kg` でも置き換えられる前のもの）。

### 知識グラフの図（G30、D-60）

`tsuzuri::diagram::render(&Input, &dyn Sources, &DiagramRequest)` は、`Input`（`KgData` の行を図が読むキーだけに写したもの。テストは付録の JSON Lines から `from_texts` で作る）から図を作り、ファイルを開きません。`module`・`calls`・`concurrency` は 1 つの `Graph`（元の ID を鍵にした `BTreeMap`。番号は鍵の byte 順）から Mermaid と DOT の 2 つの書き手で出し、`types` は classDiagram、`ownership`・`layout` は Markdown です。ノード数が `--max-nodes` を超えると `graph::fold` がグループ（モジュール）ごとにまとめ（辺の多いグループから残し、残りは 1 つ。呼び手は保つノードの外に 1 つの余地を残す。`concurrency` の区画とセルが収まらなければ `diagram_too_large`）、`types` のクラスも同じ規則でまとめ（Mermaid と DOT が `types::grouped` を共有）、`module` の循環は `kg::algo::strongly_connected` で求めます。辺は 400 本（`ownership` は状態図ごと、`transitions` と `types` の DOT も）、本文は 45,000 文字まで（超えたらラベルの上限を半分にして 1 回作り直す）で、省いたときは `folded` です。`transitions` だけは `Sources`（CLI と MCP は `ProjectSources`。プロジェクトが読み込んだソースを、知識グラフのモジュールの `path`（依存のものは名前空間とファイルの論理的なパス）で引き、ほかのファイルは開かない）でソースを読み、`manifest.json` の `modules[].source` の SHA-256 と照合してから `parse_with_source_all` の構文木の `match` の腕を辿ります。`kg::build::checked_files` は、カードの後に同じ中間の `KgData` から `diagram::views` を作って `views/*.mmd` を足します（`kg-views` の段。失敗した図は足さない）。生成した Mermaid は `validate_mermaid`（生成する行の形だけを受理する）と、テストの外で Mermaid 12.1.0 の `parse` を通します。

### 公開 API の差分（G19）

`tsuzuri api-diff`（`src/api_diff.rs`）は、`Project::load_for_api`（依存の git・registry を取得せず、`Tsuzuri.lock` も読まない `load_for_docs` の変種）で 2 つの入力を読み、root パッケージ（`Project::root_package`）のソースを `parse_with_source` だけで構文解析して、公開宣言を「完全名と種類 → 項目」の表にします。署名は `docgen` の描画（`signature`、`type_text` など）を共有し、比較用には、型変数と region を最初に出てきた順の連番に置き換え、制約を並べ替え、通常の矢印だけを平坦化し、純粋な戻り型と factory の呼び出し段階を保った正規形（`Renaming`、`flatten`）を使い、報告にはそれぞれの版の書かれたままの形を使います。`add_items` は `Program` のすべての field を名前付きで分解するので、`Program` に宣言の種類を足すとここが compile error になり、API に入れるかどうかを決めることになります。`compare` は変更を breaking 先、名前順に並べるので出力は決定的で、表示は 1000 件までです。既定では型検査もコード生成もしないので、IR には作用しません。`--effective` では `Project::load_for_docs` と完全な解析を使い、通常の IDE index では省略する `SemanticIndex.resolved_api` を明示的に要求します。`src/semantic_api.rs` は所有権による Copy 推論と class 制約の固定点伝播が終わった後、`polymorph::specialize` が generic template を消費する直前に明示要求の snapshot を保存します。通常の semantic index と非推奨警告は従来の時点で収集し、API の要求だけをこの完了段階へ遅らせます。保存するのは generic な署名、展開した alias、型・長さパラメーター、checker が持つ region の対応と呼び出し側の制約で、内部 package ID を公開名へ戻します。関数の class 制約は `CheckedFunction.constraints` を既存の instance／superclass 正規化へ通し、inline／alias 経由の要求を含む残余の集合を保存します。具体型ですでに満たされた要求は残さず、モジュール関数は `CheckedFunction.members` の推論・伝播済みの署名を保存します。具体 receiver は `solve_members` が解決済みなので除外し、署名なしの宣言が同じ receiver/name の型付き要求で精密化されていれば型付きの側を使います。公開 alias も既存の alias 展開と inline 制約抽出・class 正規化で独立に要求を保存し、ローカルの使用箇所に依存しません。比較時には型変数の改名後に整列・重複除去します。pure な generic template は特殊化ごとの効果・所有条件の証明義務も checked contract に保存し、ABI が同じでもその新設を保守的に breaking とします。非公開 helper 経由の残余義務も `GenericFunction` の型引数対応に沿って呼び出し側へ伝播します。型パラメーターを頂点にした逆依存グラフの worklist で各頂点を一度だけ処理し、相互再帰を固定点まで解きます。通常の generic 関数内の明示的な純粋 callback も、値の型・capture・実行する本体に現れる型変数を同じグラフの始点にします。実行する本体は既存の純粋性 scanner を共有して求め、実際に実行する task・並列 callback・閉じたローカル callable を含めます。返す通常 callback の本体は後の実行と区別します。通常の効果検査では型変数の収集を要求しません。具体化したパラメーターの義務を別の generic 引数へ混ぜず、公開署名内の型変数の出現位置の mask を保存して改名でも比較を保ちます。class の既定実装と instance のメソッドにも残余義務を保存し、instance の型引数を代入してから比較します。instance 内はメソッド名で整列し、class の宣言順では変わりません。既定実装の新設自体は従来どおり non-breaking です。`--effective` では、未使用 template 内でも完全に具体化した generic 参照、静的に選ばれる class／演算子の呼び出し、具体 instance のメソッドと既定実装を既存の `Specializer` の検証根に追加します。途中の通常 helper も含め、実際の所有条件と選択した class メソッドの効果まで検査してから充足と扱います。不正なら API の一部だけを返さず解析エラーにします。この追加の検証根は明示の API 収集時だけで、通常の型検査・特殊化の根やコード生成は変えません。型変数名や source alias の文字列には依存せず、既定の構文比較と実際の型検査・特殊化は変えません。ソースの表示は置き換えません。標準ライブラリそのものを root とする場合以外は依存の宣言を API に含めません。依存や解析のエラーでは診断を返し、構文比較へフォールバックしません。

### bindgen

Clang の target は Linux x64/Arm64 と macOS Arm64 の LP64 が対象です。`lp64_target` は Intel macOS も拒否し、`bindgen_driver` は `E2002` を返します。Rosetta で起動する Intel macOS の Clang を、Apple Silicon 用の型として扱いません。

`tsuzuri bindgen`（`src/bindgen.rs`）は lexer・parser・check・LLVM を通らず、生成したテキストは利用者のソースとして通常の経路で検査されます。生成するのは C の ABI がホスト ABI と一致すると確かめられる宣言だけで、型は Clang の表記（`qualType`）を typedef 展開してから固定の表と完全一致で照合し、表にない表記は推測せず `W2002` で省きます。`desugaredQualType` は typedef とともにその alignment 属性を落とす（`typeof` の先の over-aligned な typedef が素の整数に見える）ので使いません。enum・typedef・struct・フィールドの属性は、配置と呼び出し規約を変えないと分かっているものの許可リスト（`HARMLESS_ATTRIBUTES`）で判定し、それ以外の属性を持つ型は変換しません。C の tag と typedef 名は別の名前空間ですが、Clang は `typedef struct { ... } S;` の型を `struct S` と表記するので、無名の struct・union を名付ける typedef は展開せず、無名の enum の typedef は同じ名前の enum の tag（入れ子の宣言も含めて全ノードから集める）があれば変換しません。
Clang は JSON の位置に `file` を変化時にしか書かず、`serde_json` の `Value` はキー順を保たないので、宣言の所属ファイルは `loc`（spelling → expansion）、`range`（begin → end）、子（`array_filler` → `inner`）の順をコードで固定して全ノードを訪ねて求めます。Clang は `sqrt` などの library builtin を最初の言及で暗黙に宣言するため、`previousDecl` が暗黙の宣言を指す関数は最初の宣言として扱います。enum 定数の値は `ConstantExpr` の値に `ImplicitCastExpr` の整数変換を適用して求めます。
マクロは AST に現れないので、4・5 回目の Clang の起動 `-E -dD` と `-E -dM` の出力を読みます。行マーカー（`# 12 "/path/x.h" 2`。ファイル名の C エスケープを戻す）が次の行のファイルと行番号を与え、それ以降の各行は次のソース行なので、各 `#define` をファイルと行に結び付け、`#undef` と再定義を反映した最終状態のうちヘッダーのものだけを、ヘッダーのバイト列の行頭表からの位置で宣言と同じ順に並べます。`-E -dD` は `#pragma push_macro`／`pop_macro` を出力しないので、`-E -dM` の最終的なマクロ表と置換列が一致しない定義は生成しません（整数リテラルに見えれば `W2002`）。整数リテラルの型は C17 6.4.4.1 を LP64 に当てはめて決め、単項マイナスはその型で計算します。
不透明なハンドルは、ヘッダーが最初に宣言し、翻訳単位のどこでも（入れ子の宣言も含めて）定義されない struct だけです。定義のある struct は呼び出し側が確保しうるので、ハンドルにしません。`--buffer`・`--consume` の注釈は引数の名前か位置で解決し、ヘッダーにない関数・引数や重なりは `E2000`、型が buffer の ABI（要素への const ポインターと直後の 64-bit の要素数）やハンドルに合わない組は `W2002` です。
出力は AST、ヘッダーのバイト列、Clang の版とターゲットだけの関数で、時刻・絶対パス・環境変数を含まず、匿名の tag の表記（`(unnamed struct at /path:1:2)`）からも絶対パスを取り除きます。コメントへ入る文字列は 0x20–0x7E 以外を `?` に置き換え、生成コードへの行の注入を防ぎます。検証は `tests/bindgen.rs`（Clang を起動しない AST 単位のテスト）と `tests/bindgen.mjs`（golden、決定性、`check`／`fmt --check`、C ライブラリとリンクする native の `-O0`／`-O3` の往復、CLI と出力保護）です。

### Windows MSVC

`native_compile_args` はコンパイラが対象とする CPU アーキテクチャに応じて `x86_64-pc-windows-msvc` または `aarch64-pc-windows-msvc` を選択し、POSIX 向けフラグと明確に分離します。Win32 タスクアダプタは、既存のスケジューラに対して SRWLOCK、CONDITION_VARIABLE、INIT_ONCE、CreateThread、WaitForSingleObject、CloseHandle による同期・スレッド機能を提供します。
`windows_abi` は型検査済みのエクスポート関数一覧にのみ `dllexport` を付与し、標準出力への書き込みは MSVCRT の `_write` による 32-bit カウントおよび戻り値から安全にサイズ拡張して扱います。UTF-8 のバイト列を損なわないよう標準出力のファイル記述子をバイナリモードに設定し、システムのコードページは改変しません。
Windows 環境にも 128-bit 整数のヘルパー関数群が同梱されており、MSVC CRT のみで安全にリンク可能です。CPU ディスパッチは Windows 上では移植性の高いベースライン実装に固定されます。
Rust 内部でのファイル同一性判定には Windows 専用の安全な同一ファイル比較 API を使用し、unsafe の禁止ポリシーを維持します。`fs::rename` による既存出力の安全な置換とハードリンク保護は Windows 専用テストで検証されています。
COFF におけるランタイム埋め込みオブジェクトの結合は `E2002` エラーとなります。実行ファイルの生成を優先し、LLVM IR とランタイムを一度だけ明示的にリンクする経路を維持します。
子プロセスへ渡すパスは、`fs::canonicalize` が返す `\\?\` 付きのものではなく、`cache::real_path` で外した形にします（共有の `\\?\UNC\server\share` は `\\server\share` になります）。Clang は verbatim パスの隣の引用 include を解決できず（`task_runtime_source_for` が `task-windows.h` を埋め込む理由）、Git for Windows は `GIT_CONFIG_GLOBAL` に verbatim パスを渡されると設定を読めません（`fetch.rs` の `Repository::create`）。`task-windows.h` は `WIN32_LEAN_AND_MEAN` と `_CRT_SECURE_NO_WARNINGS` を、コマンドラインにないときだけ定義します（`-Werror` と `-D_CRT_SECURE_NO_WARNINGS` の組で再定義を警告にしないため）。
native のトラップは、`llvm.trap` が STATUS_ILLEGAL_INSTRUCTION（0xC000001D）でプロセスを終わらせます。シグナルは使わないので、`tsuzuri run` は `E2005`（`exit code: 0xc000001d`）、`tsuzuri test` と `bench` は `trapped or exited with code -1073741795` と報告します。トラップの位置はプログラムが先に標準エラーへ書きます。
検証は `.github/workflows/windows.yml`（workflow `Windows MSVC`、job `windows` の matrix、x64 は `windows-2022`、Arm64 は `windows-11-arm`）が行います。LLVM 21.1.8（CPU ごとの installer と SHA-256 を固定）、MSVC 14.44、Windows SDK 10.0.26100 を使います。Visual Studio は x64 が 2022、Arm64 が 2026（インストール先は `18`）です。`ilammy/msvc-dev-cmd` は最新の Visual Studio の `arch`・`toolset`・`sdk` を選び、`Show tools` が MSVC・SDK の版と Visual Studio のインストール先を検査します。`cargo fmt`・`cargo clippy`・release build、`tests/windows.mjs`（native の `-O0`／`-O3` の UTF-8、出力の置換、`examples/tasks`・`examples/control`、Win32 scheduler の 4 つの形と失敗の注入 3 種、`dllexport` の IR と C host、COFF の `E2002`、hard link の `E2003`、object の ABI、WASM の import なし、ホストの MSVC triple の build・run・test、トラップ）、`tests/cache.mjs`、`tests/examples.mjs`、`tests/wasm_threads.mjs`、`cargo test` を実行します。`tests/windows.rs` の cross-link は、`TSUZURI_WINDOWS_SDK` を与えたときだけ MSVC の SDK で COFF/PE を作ります。MinGW の配布物は `vscode.yml` の両 CPU の job が検証します。DLL と OS API は未対応です（[G10](../_features/_completed/G10-windows.md)）。

### Target と Platform（G15）

`--target TRIPLE` は、`src/target.rs` の閉じた表（5 環境の 9 個の native の `Triple`）から選び、`BuildOptions::triple`・`TestOptions::triple`・`BenchOptions::triple` に入ります。対応は Windows x64/Arm64、Linux x64/Arm64、macOS Arm64 だけです。Linux の glibc/musl と Windows の MSVC/MinGW を区別します。それ以外の target は `E2000`、コンパイラのビルドホストは `src/lib.rs` の `compile_error!` で制限します。ビルド先の性質は `cfg!` を直接読まず `Platform` に尋ね、triple がなければ `Platform::HOST` が従来と同じ答えを返すので、対応環境の `--target native` の生成物は変わりません。WebAssembly 出力は維持します。
IR が target で変わるのは、Windows の COFF の allocator と `windows_abi`、`@cpu` の level（`Platform::cpu_levels`）、`Math.fma` の命令（`Platform::hardware_fma`）の 3 か所です。`EmitOptions` に target を足すと `tests/windows.rs` などの多数のリテラルが壊れるので、IR を作る間は `PlatformScope`（スレッドローカルの guard）が呼び出し元の `Platform` を運びます。
Clang の引数は、`compile_args`（`--target=` と `-fPIC`）、`sdk_args`（ランタイムの C のコンパイルとリンク。`TSUZURI_SYSROOT` を `--sysroot`、macOS は `-isysroot` と `-Werror=incompatible-sysroot` にする）、`link_args`（ビルド機と違う target に `-fuse-ld=lld`）、`final_link_args`（`link_args` と、`TSUZURI_LINKER=lld` のときの `-fuse-ld=lld`。実行ファイル・`-g` の別リンク・`--emit shared` の最終リンクだけで、`-r` のリンクには使わない。値の検査は `build_complete` の冒頭で行い、成果物キャッシュのキーには未設定・`default` 以外のときだけ入る）、`no_sdk_args`（SDK を読まない IR のコンパイルと `-r` のリンク）に分け、`HOST` では `TSUZURI_LINKER` が未設定ならすべて空です。ランタイムの選択（`os.c`、`net.c`、`async.c`、`task-windows.h`）は `Platform::os_apis`・`net`・`reactor` と `task_runtime_source_for` が target で決めます。Windows の OS API は `E2002` で拒否し、推測してリンクしません。
ビルド機で動かせない target の `run` と `test` は、`TSUZURI_RUNNER` の前置コマンド（`ForeignRunner`）を通し、なければビルドの前に `E2000` です。`test --coverage`、`test -g`、`bench` は拒否します。同梱ツールチェーンの `scripts/toolchain/clang.rs` は、`--target=<Clang の target>` を Zig の target に写す表（`NATIVE_TARGETS`）を持ちます。`--target=` がないときの挙動は変えていません。
同梱 launcher のホスト検査は target の分岐前に行い、明示した WASM や別 OS の target でも Intel macOS のホストを拒否します。Intel macOS の Clang target も、どのホストからも拒否します。CLI・VSIX 共通の `scripts/toolchain/bundle.mjs` は、作成時と再検証時に Clang の `-dumpmachine` が Apple Silicon（`arm64-apple-*` / `aarch64-apple-*`）であることを確認します。CI と VS Code の macOS ホストは `darwin-arm64` だけです。CPU・f16 の x64 生成コード検査は、対応する Linux / Windows 向けを使い、Intel macOS 向けを作りません。
検証は、`tests/targets.rs`（target ごとの IR と object の形式、キャッシュ、偽の Clang と実行器による `run`・`test`、launcher の表）、`tests/targets.mjs`（9 triple の実行・リンク・object、削除した target の `E2000`。`TSUZURI_CROSS_LINK=1` で同梱ツールチェーン、`TSUZURI_CROSS_EXECUTE=1` で qemu-user）、`tests/platforms.mjs`（配布物と CI matrix が 5 環境だけを含むこと）です。CI は `.github/workflows/targets.yml` の 3 host と `windows.yml` の 2 host が実行し、`vscode.yml` は 5 環境の配布物を作ります。tier の記録は [対応プラットフォーム](../_tsuzuri/language-reference/compiler/platforms.md) の表だけです。

### 結果付き Task

`Task.parallel_results` は通常の組み込み関数の枠組みとコールドなタスククロージャを経由し、専用の `TypedExprKind` として `llvm_task.rs` へ下げられます。
ランタイム ABI は `i64 tsuzuri_task_parallel_results(i32 (*run)(context, index), context, length)` で定義され、戻り値 `-1` は全タスクの成功、それ以外の非負値は最初に失敗したタスクの最小インデックスを表します。
ネイティブおよびスレッド対応 WASM の既存タスクグループ構造体に結果実行関数と失敗追跡フィールドを追加し、キューのロック内で未配布の長さを短縮して残存カウントから未開始分を減算します。開始済みタスクの残存数が 0 になるまで呼び出しスレッドは待機を継続します。
LLVM は一時的な `Result` 配列と、タスク開始フラグを記録する `i8` の `started` 配列をスタック上に確保します。コールバック関数は環境を消費して実行結果を書き込んだ後に `started` フラグを公開し、ジョイン（合流）後はそのフラグを用いて「初期化済みの結果」と「未開始のクロージャ」を厳密に区別します。
成功時のペイロードは出力配列へ移動され、エラー時は最小インデックスのエラー値のみが戻り値へ移動され、その他の値は既存の `drop` 処理によって確実に解放されます。一時 `Result` 配列全体に対するデストラクタ呼び出しは行わず、生のバッファのみを解放するため、移動済みのスロットが二重解放されることはありません。
再帰的な `Result` 型の外側ノードも、ペイロードの移動完了後に解放されます。トラップ情報（trap-info）は既存のコールバック ABI を維持し、タスクの実行コードに対してのみコンテキストを伝播させます。
テストは `tests/tasks.mjs` および `wasm_threads.mjs` により、41 件の実行結果、4 件のトラップ、所有配列・クロージャ・再帰ペイロードの取り扱い、結果の順序性、最小エラーインデックスの選択、およびコールド破棄が検証されています。C 言語のスケジューラは ASan／UBSan／TSan を適用した環境でも安全性が確認されています。

### HKT（高階型）

`ClassDecl.kind` は明示的なカインド情報を保持し、`TypeExprKind::Apply` のヘッド識別子が `'f` で始まる場合は型変数への適用として扱われます。既存の名前付きヘッドおよびフォーマッタの経路はそのまま維持されます。
クラスのコンストラクタのアリティ（引数の数）とシグネチャ・制約から導出されるカインド環境を参照し、値型の出現位置には型の完全な飽和（全引数の適用）を要求します。初版におけるカインド引数はすべて通常の `Type`（`*`）に限定され、高階カインド引数や型別名の引数は拒否されます。
`Type::Partial` はコンストラクタ宣言と末尾固定引数を保持し、`Application` はヘッドと適用引数列を保持します。`Type` 構造体全体のサイズは 4 ワードに抑えられており、`map_type`、`substitute`、および `Inference::resolve` の各処理において型が飽和した段階で、既存の Record、Union、Array、List、Vec、Task などの具象型へ正規化されます。
HKT メソッドの解決では、クラスのレシーバ型と固有の値型変数をフレッシュな型変数として生成し、具象インスタンスの完全なシグネチャからメソッドの特殊化引数を決定します。デフォルトメソッドの実装も、既存の内部関数および共有の `Specializer` を利用して展開されます。
インスタンス宣言のヘッドとメソッド定義で同名の型変数が使われている場合は、生成されるメソッド関数の型変数をアルファ変換によって分離し、名前の意図しない捕捉を防止します。
値の型表現に `Partial` や `Application` が残存したまま LLVM コード生成へ到達した場合は、入口の整合性検査で `E1015` エラーとなります。名前付きジェネリクス宣言における通常の型変数（`Type::Variable`）は従来どおり維持されます。
検証は `tests/higher_kinds.rs` および `tests/features.mjs` の `higher_kinds` スイートにより、Maybe、Result、デフォルトメソッド、ジェネリック関数、ローカル型注釈、すべてのコンストラクタ形状、および所有ヒープ領域の挙動が網羅的にテストされています。標準ライブラリに `Functor` などの型クラスを暗黙的に自動導入することはありません。

### GPU Phase 1

`gpu.rs` は、通常の型検査および単相化が完了した型付き IR を走査し、既知の関数呼び出しからなる有向グラフを抽出します。コンパイラ本体とは独立した型推論器や数値評価器を新設することはありません。
`GpuKernel.cpu_reference` は既存の LLVM emitter を再利用して生成されます。`Gpu.init` および `Gpu.map` のコールバックに対する各種制限はクロージャの lowering 後に検査され、ファーストクラスの関数 API を経由した制限の回避は確実に拒否されます。
`std/Gpu.tz` は明示的な `CpuReference` のみを構築します。不透明な `Device` および `Buffer` が非 Copy（所有型）である性質は、型システムとシンボリック所有権検査の両面で統一的に扱われ、内部フィールドへのアクセスは既存の不透明型保護ゲートによって遮断されます。
WGSL コード生成では、各部分式が順序付きの一時 `let` 変数へと評価され、`if` や短絡論理演算の分岐内に存在する式はその分岐の内部でのみ評価されます。同一ビット幅の整数キャストには `bitcast` を用い、符号付き算術は `u32` のラッピング演算を経由して表現します。
WGSL の型制約および浮動小数点・トラップの言語契約に基づき、シェーダー内での `i64`、`f64`、厳密な浮動小数点演算、除算、剰余算の使用は拒否されます。CPU 参照実装での許容とシェーダーコード生成での許容を混同することはありません。
CLI の `--emit wgsl` オプションは、単一のエクスポート関数を持つカーネルプロジェクトを通常の安全な出力保護経路を通じて公開します。WebGPU ホストの試作実装は、デバイスの各種制限（limits）、シェーダー診断情報、所有バッファ、エラースコープ、およびコマンドキューの同期を処理し、GPU 実行の明示要求を勝手に CPU 実行へ縮退させることはありません。
LLVM において列挙型のスカラー別名は前方参照できないため、すべての enum 別名は record／union 構造体の定義よりも前に先行して生成されます。構造体同士の相互前方参照は従来どおりサポートされます。
検証は `cargo test --test gpu` および `tests/gpu.mjs` で行われます。`TSUZURI_WEBGPU=1` 環境変数設定時には、実際の GPU アダプタ上でシェーダー実行、初期化／マッピング、常駐チェーン、および境界の動作がテストされます。

### GPU Phase 2

`Gpu.WebGpu` を構築するユーザー関数が 1 つでもあるプログラムだけが「デバイス対応」で、そうでないプログラムの IR・import・ABI は変わりません（`src/gpu_devices.rs`）。単相化の前に `request`・`init`・`map`・`init_relaxed`・`map_relaxed` の呼び出しを std の `request_on`・`init_on`・`map_on` へ付け替え、呼び出しの最後に整数のマーカー引数を足します。単相化と closure lowering のあとで、呼び出し箇所のコールバックごとに、`extract_kernel` から厳密または緩い WGSL を作って `CheckedModule.gpu` の表へ入れ、マーカーをカーネル番号に書き換えます（厳密な WGSL に出せない呼び出しは番号なし）。表は（コールバック、厳密か緩いか）の順に決定的に並び、LLVM の定数 `@tz.gpu.kernels` と WGSL テキストとして埋め込まれます（`src/llvm_gpu.rs`）。
`std/Gpu.tz` の `request_on`・`init_on`・`map_on` は、std 専用の組み込み関数 `Gpu.__open`・`Gpu.__features`・`Gpu.__run` を呼びます。この 3 つの関数自体も `private` で、利用者のコードが呼ぶと `E1022` です。カーネルの番号はコンパイラだけが書くので、利用者が別のコールバックのカーネルを、型の合わない配列へ当てることはできません。念のため、`Gpu.__run` の lowering は、番号のカーネルの lane の種類（入力・出力）を、呼び出しの要素型の種類と比べ、合わなければ「カーネルなし」としてホストにソースを渡しません（ホストは lane の種類で配列の大きさを決めるので、合わないカーネルは配列の外を読み書きします）。ランタイムの境界は 2 つの C 関数で、`tsuzuri_gpu_open(backend, features)` は 0 か利用不可などの状態を、`tsuzuri_gpu_run(backend, mode, flags, lanes, wgsl, wgsl_length, spirv, spirv_length, input, count, output)` はカーネル 1 回の実行を表します（0 成功、1 利用不可、2 機能不足またはカーネルなし、3 上限超過、4 実行時エラー。0 以外は理由を 1 行出してトラップ）。native は `src/runtime/gpu.c` が定義し、WebAssembly は `--wasm-feature webgpu` のときだけ import にし、そうでなければ状態 1 を返すモジュール内の定義に置き換えます。`backend` は `Gpu.Backend` の番号で、`spirv` の欄は Vulkan（Phase 3）が使います。
`src/runtime/gpu.c` は、`dlopen` した wgpu-native 29 の WebGPU C API の必要な部分だけを自前で宣言し、バージョンを確かめてから、パイプラインを WGSL ごとにキャッシュして、呼び出しごとにバッファ作成、アップロード、実行、読み戻しをします。ライブラリは、名指しされたものを除いて、システムの場所の絶対パスだけから探します（ディレクトリのない名前は、macOS の `dlopen` が作業ディレクトリを探すので使いません）。上限は、開いたデバイスから `wgpuDeviceGetLimits` で読み、超える呼び出しは何も作らずに状態 3 にします。オブジェクトは作るたびにエラーの有無（uncaptured-error コールバックで受けた状態）を確かめ、無効なものは使わず、シェーダーモジュールやパイプラインとしてキャッシュにも残しません。wgpu-native は無効なオブジェクトを送信すると panic でプロセスを異常終了するためです。完了待ちはブロックしない poll だけで回し、最初の 5 ms は続けて poll し（wgpu-native 29 の Metal では、ブロックする poll で待つと、0.35 ms ほどで終わる小さなカーネルの呼び出しが 1.5 ms 前後かかったため。原因はソースで確かめていない）、それを過ぎれば poll の間に短い sleep を挟みます。ブロックする poll は 60 秒の打ち切りが効かないので使いません。待ちの状態は静的な 1 つで、世代番号を持ちます。打ち切った待ちのコールバックが後から動いても、何も書き換えません。読み戻しが終わらなかったデバイスは使わず、終了時に解放もしません。WebAssembly のホストは、同じ境界を JSPI で中断する関数として `createGpuImports` が実装します。
検証は `tests/gpu_runtime.mjs` で行います。ローダー（ライブラリなし、空、名指しのパスがない、バージョン違い、`wgpuGetVersion` なし、関数不足、バージョン 0 の扱い、作業ディレクトリのライブラリを読み込まないこと）と WebAssembly の import は、GPU なしで検査します。native のランタイムは、C のホスト（`tests/gpu_runtime_host.c`）で直接動かし、偽の wgpu-native（`tests/gpu_runtime_fake.c`。無効なオブジェクトを返し、それを送信すると異常終了し、ブロックする poll は応答しないデバイスで戻らない）に、バッファ・バインドグループ・パイプライン・コマンドバッファの失敗、デバイスの喪失、応答しないマップとアダプタ、デバイスの上限を注入して、どれも状態で終わることを確かめます（`TSUZURI_SANITIZE=1` で AddressSanitizer と UBSan を付けます）。WebAssembly のホストは、偽の WebGPU プロバイダ（`tests/gpu_runtime_provider.mjs`）で、2 GiB を超えるアドレスと状態 3・4 の使い分けを確かめます。`TSUZURI_WEBGPU=1` では、native は wgpu-native、WebAssembly は Dawn（JSPI）で、i32・i32u を CPU 参照とビット単位で、緩い `f32`・`f16` を許容誤差で比べ、デバイスでの実行回数を `TSUZURI_GPU_DEBUG` の出力で数え、C のホストを実機の wgpu-native でも動かします（壊れたシェーダー、入口のないパイプライン、レイアウトに合わないバインドグループ、上限の境界）。

### GPU Phase 3

SPIR-V は `src/gpu/spirv.rs` が、`extract_kernel` の結果から決定的に作ります（SPIR-V 1.3、Vulkan 1.1 が基準）。`Shader` capability、`Logical`/`GLSL450`、`GLCompute` のエントリーポイント `map_main` と `init_main`（`LocalSize 256`）、`ArrayStride` を持つ StorageBuffer のブロック、push constant `{length, base}` の形で、64 bit の型には `Int64` を付けます。厳密な `f32` のモジュールは、すべての浮動小数点演算の結果に `NoContraction` を付け、`SPV_KHR_float_controls` の `SignedZeroInfNanPreserve`・`DenormPreserve`・`RoundingModeRTE`（幅 32）を宣言し、`f32` から整数へのキャストを範囲の比較と `OpIsNan` の選択に展開します。厳密な `f32` の `/` は `OpFDiv` の誤差（2.5 ULP）のため `E1018` で、整数の `/`・`%`・`**` と `f64`・`f16`・`bool` の lane も出しません。LLVM の SPIR-V ターゲットは使いません。LLVM 23 は float controls の execution mode は出しても `NoContraction` を出さず（`contract` フラグの有無に関わらず 0 件）、Vulkan の仕様は `NoContraction` のない演算の積和と再結合を許すからです。
`src/gpu_devices.rs` は、`Gpu.WebGpu` を構築するプログラムには WGSL を、`Gpu.Vulkan` か `Gpu.Auto` を構築するプログラムには SPIR-V を、呼び出し箇所ごとのカーネルの表へ入れます。どれも構築しないプログラムの IR、import、ABI は変わりません。表の記述子は `{flags, lanes, features, wgsl, wgsl_len, spirv, spirv_len, weight}` で、`features` は WebGPU の `shader-f16`（ビット 0）、Vulkan の `i64`（ビット 1）と厳密な `f32`（ビット 2）、`weight` は 1 lane が実行する命令の数の下限です（分岐のないコードは全部の命令、`if` は条件と安い側の枝、`&&`・`||` は左の項だけ、呼ぶ関数は同じ規則で数えた重み。1 から 2,147,483,647 までで、欄が `i32` なので、それより重いカーネルもこの値です）。`std/Gpu.tz` の `init_on`・`map_on` は、呼び出しごとに組み込み関数 `Gpu.__select` で実行するバックエンドの番号（0 が CPU 参照、1 が WebGPU、2 が Vulkan）を決め、0 以外なら `Gpu.__run` がその番号で動かします。`Auto` 以外のデバイスは、自分のバックエンドをそのまま返します。選んだ番号は、プロセスで 1 つの atomic の値（`@tz.gpu.last`）に記録され、`Gpu.__last` が読みます（`Gpu.last_backend`）。WebAssembly の `Gpu.__select` は常に 0 を返し、import を足しません。
`src/runtime/gpu-vulkan.c` が Vulkan のバックエンドです。実行ファイルのリンクでは、IR にマーカー `; tsuzuri-gpu: vulkan` があるときだけ、`#define TZ_GPU_VULKAN 1` のあとに `gpu.c` の前に置かれます（`runtime_source`）。Vulkan の C API は、必要な部分だけを自前で宣言し、`dlopen`／`LoadLibrary` で読み込みます。`TSUZURI_VULKAN_LIBRARY` があればそのパスを書いたとおりに、なければ決まった候補だけを順に試し（macOS は `/opt/homebrew/lib` と `/usr/local/lib` の絶対パス、Windows は `LoadLibraryExA` の `LOAD_LIBRARY_SEARCH_SYSTEM32`、Linux は動的リンカーが作業ディレクトリを探さない soname）、作業ディレクトリは探しません。`vkGetInstanceProcAddr` を持たないライブラリは `dlclose` して次へ進みます。ローダー、インスタンス（portability 列挙を含む）、物理デバイスの列挙（`tz_vk_enumerate`。種類、機能、制限、メモリの種類を候補の表に保存し、論理デバイスは作らない）、論理デバイス（`tz_vk_create_device`）、バッファとメモリ、descriptor set、パイプライン、コマンドバッファ、フェンス、読み戻しまでを、どの失敗の経路でも解放します。デバイスはプロセスに 1 つで、最初に来た要求が決めます。明示的な要求（`tz_vk_pick_explicit`）は、計算キューを持つ Vulkan 1.1 以上のデバイスから、動かせる、必要な機能を持つ、種類（離散、統合、仮想、その他、CPU）の順に選び、同順位なら先に列挙されたものです。`Gpu.Auto`（`tz_vk_pick_auto`）は、統合 GPU・共有メモリ・機能・上限の確認を、`vkCreateDevice` の前に、デバイスが報告する内容だけで行い、合うデバイスがなければ何も作りません。能力は `shaderInt64`、`VkPhysicalDeviceFloatControlsProperties`（幅 32 の 3 つと独立性）、ワークグループとストレージの上限、`maxMemoryAllocationSize` を明示的に確かめ、ステータスを 1（利用不可）、2（機能不足）、3（上限超過）、4（実行時エラー）に対応させます。初期化はプロセスで 1 回、ロックの下で遅延して行います。ロックは 2 つで、取る順は状態、実行です。状態のロック（`tz_vk_mutex`）は、デバイスの状態、能力、パイプラインの表、`Gpu.Auto` の貯金を守り、フェンスの完了待ちのあいだは持ちません（例外は、厳密な `f32` のプローブを最初に動かすスレッドで、1 回だけ入れ子で待ちます。明示的な要求は、別のスレッドのカーネルが動いていればその完了も待ちます。`Gpu.Auto` の判断は、実行のロックを `TZ_VK_EXEC_TRY_LOCK` で試すだけで、取れなければ `TZ_VK_BUSY` を受け、プローブの結果を記録せずに CPU 参照で動かし、貯金から引いたコンパイルの分を返します）。実行のロック（`tz_vk_exec_mutex`）は、descriptor set、コマンドバッファ、フェンス、キューを守り、1 回の実行のあいだ持ちます。`tz_vulkan_run` は `tz_vk_prepare`（状態のロック）と `tz_vk_execute`（実行のロック）に分かれ、`struct tz_vk_plan` がパイプラインと能力の写しを渡します。パイプラインの表は 256 個までで、それを超えるカーネルは、plan が持つ一時のモジュールとパイプラインで動かし、`tz_vk_execute` の終わりに解放します。`VK_ERROR_DEVICE_LOST` は `poisoned`（atomic）を立て、以後そのデバイスを触りません。厳密な `f32` は、デバイスがプロパティを報告するだけでは許さず、最初の要求で 27 lane の適合プローブ（`tests/gpu_vulkan_probe.spvasm` を組み立てた SPIR-V）を 1 回動かし、CPU 参照のビット列と比べて、通ったときだけ許します。`Gpu.Auto` の規則は `tz_vulkan_auto` にあり、`gpu.c` の `tsuzuri_gpu_select(mode, lanes, features, spirv, spirv_length, weight, count)` が呼びます。式だけで決まる見積りはロックなしで行い、CPU 参照と答える呼び出しは、実行中のカーネルを待ちません。それ以外は、状態のロックを待たずに試し、取れなければ CPU 参照です。`gpu.c` は、`backend == 2` を Vulkan のバックエンドへ回し、WebGPU のロックは通しません。
検証は、SPIR-V の構造・決定性・`spirv-val`・実機での実行（`tests/gpu_spirv.rs`）、SPIR-V と記述子の埋め込みと、`Gpu.__run` の lane の種類の検査が Vulkan と `Auto` の腕に効くこと（`tests/gpu_vulkan_lowering.rs`）、ランタイムのハーネスと合成した Vulkan ライブラリ（`tests/gpu_vulkan_runtime.c`、`tests/gpu_vulkan_mock.c`。デバイスごとに種類・名前・機能を変えられる）でのローダー（作業ディレクトリに置いたライブラリを読み込まないこと、`vkGetInstanceProcAddr` のないライブラリの読み飛ばし）・デバイスの選択（順位、機能の優先、ハイブリッド構成、デバイスを作らないこと）・能力・上限・失敗の網羅（N 番目の呼び出しが失敗する掃引、ASan・UBSan・TSan）・デバイスの喪失・パイプラインの表の溢れ・実行中の待ちと `Gpu.Auto` の判断の独立（厳密な `f32` のプローブが要る判断、明示的な要求の待ち、貯金の返却を含む）と `Gpu.Auto` の規則（貯金の消費を含む。`tests/gpu_vulkan.mjs`）、言語としての実行と重みの下限と WebAssembly の確認（`tests/gpu_vulkan_language.mjs`）で行います。測定は `benchmarks/run-gpu-vulkan.mjs` です。

### モジュール

「1 ファイルにつき 1 モジュール」を強制し、モジュール名はファイル名から自動的に導出されます。
`Project::load_for_docs` で標準ライブラリ自身を読むときも、Std の解析入力には全 OS 共通の仮想名 `std/Name.tz` を渡します。実ファイルのパスは保持し、仮想名の区切りを Windows の `\` に変えません。
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

### 名前空間

パーサーは、ファイル先頭の文脈キーワード `namespace A::B` およびそれに続く `using A::B` を読み取り、それぞれ `Program.namespace` と `Program.usings` に格納します。
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

### パッケージ

パッケージ読み込み層は、`Tsuzuri.toml` で定義されたローカルパス依存、commit 固定の git 依存、registry の版の要求を取り扱います。`package.rs` が限定的なマニフェスト文法、`Tsuzuri.lock` の読み書き（`parse_lock`／`render_lock`）、内容ハッシュ（`content_sha256`）を受け持ち、ドライバーの `load_packages` は明示的なスタックを用いて依存グラフの循環参照、パッケージ名と取得元の一意性、およびリソース上限を検査します。
`SourceFile.package` には正規化されたルートパスとパッケージ名からなる `PackageId` が格納され、依存パッケージの名前空間が各ファイルの `relative_path` に付与されます。型検査における User／Std の分類や `private` 可視性の境界判定自体は変更されません。
マニフェスト情報は `Project.manifests` に保持され、`source_for` においては通常のソースファイルの後に続く ID として参照されます。ビルド成果物およびドキュメントの出力保護機構は、ルートパッケージと依存パッケージの両集合を対象として適用されます。
すべてのソースファイルは論理パス順に整列され、標準ライブラリ（std）は常に末尾に配置されます。依存パッケージのルートは親プロジェクトのファイル探索スコープから除外され、同一ルートのパッケージが重複して読み込まれることはありません。ビルドスクリプトは実行しません。
git・registry 依存の解決は `load_packages` に渡すリゾルバーだけが異なり、グラフ走査と E04 の規則は取得とビルドで共有されます。`git` を起動しネットワークに触れるのは `tsuzuri fetch` と `tsuzuri publish`（`fetch.rs`）だけです。fetch は走査を 2 回行います。1 回目は git 依存を取得し、版の要求を集めて走査から外し（リゾルバーが `None` を返す）、index の取得と最小版選択（要求をたどる BFS、名前ごとの最大値、互換範囲の検査、選んだ版からの到達可能性）の後、選んだパッケージを取得・検証してから、2 回目の走査で全体の規則を確かめます。fetch は空の hooks と設定で隔離した一時 bare リポジトリへ commit を取得し、`ls-tree -r -z -l` と `cat-file --batch`（要求の書き込みと応答の読み出しを別スレッドで行う）でパッケージのファイルだけを読み、パス・モード・大文字小文字・上限を検査してから、ストア（キャッシュルートの `packages/git/<sha256>/`）へ同じファイルシステム内の rename で確定させます。`BuildCache::open` を先に呼んでキャッシュルートの marker を作るため、ビルドキャッシュと共存し、`evict` はストアに触れません。
ほかのコマンドはオフラインのリゾルバーで `Tsuzuri.lock` とストアだけを参照し、`load_from_root` が git パッケージのソースを読んだ後に内容ハッシュを照合します。`Tsuzuri.lock` は `Project.manifests` に加わるため、出力保護とビルドキャッシュのキーにも含まれます。

### 標準ライブラリ

`std/` 配下のソースコードは `stdlib::SOURCES` としてコンパイラバイナリ内に `include_str!` で静的に埋め込まれており、`analyze`、`analyze_modules`、および `Project::load` の各処理において、ユーザーソース群の末尾に追加されます。
パース順序は入力順に従うため、ユーザー側のソース ID や `Project.root` の値は標準ライブラリの有無によって変動しません。
Rust の単体テスト向けには `analyze_modules_with_std` が用意されており、std の内容を任意に差し替えることが可能です（空スライスを渡せば std なしの環境となります）。
各モジュールは `ModuleOrigin`（`User` または `Std`）の属性を持ち、`stdlib::RESERVED_MODULES` に定義された予約名をユーザーモジュールとして定義することはできません。
std の仮想パスは `std/Name.ext` という平坦な形式で管理され、ファイル出力保護の対象からは除外されます。これにより、ユーザーコードの `relative_path` と混同されるのを防いでいます。
無修飾の型名、case、レコード、型クラスの解決においては、まず自モジュール内、次いで完全修飾名（モジュールパスが同一名の型を表す場合を含む）を検索します。その後、参照元がユーザーコードであれば「ユーザー定義モジュール群 → std モジュール群」の順序で各段階ごとに一意な候補を探索し、参照元が std であれば std モジュール群のみを探索します。
std モジュールにおける `export def` の使用は禁止されており、std の関数を外部から呼び出す際はユーザー定義関数と同様にモジュール名による修飾が必須です。
std のソースコードは型検査の対象となりますが、`closures::lower` の処理後に到達可能性解析（reachability analysis）が行われ、不要な関数は最終成果物から間引かれます。
例外は `stdlib::OPT_IN` の opt-in std モジュール（`Alloc`、`Arena`、`Regex`、`Unicode`、`Json`、`Cbor`、`Bench`、`Gen`、`Async`、`Matrix`、`MatrixView`、`Tensor`、`Net`、`Atomic`、`Mutex`、`Channel`、`Monoids`。D-40・D-41・D-42・D-44・F14・F15）です。ユーザーのモジュールからの無修飾の解決（`Names::choose`）は opt-in std モジュールの宣言を候補にしないので、ユーザーのコードはそれらを修飾した名前でだけ参照します。
そのため `Project::load` 系（言語サーバーの `load_with_overlays` を除く。REPL の `Project::single_main` を含む）と `analyze_modules_all` は、`stdlib::sources_for` が選んだものだけを読み込めます。`sources_for` はユーザーのソースの ASCII 識別子の並び（先頭の数字を除いた部分も含む）を走査し、`OptIn::names`（モジュール名と、他所の型に instance を与える組み込みクラス。`Json` の `Encode`・`Decode`）か、`OptIn::aliases`（モジュールのビルダーの `@alias`。`Async` の `async`。小文字の語は別名と一致したものだけを数える）のどれかが現れたモジュールと、その `uses` の閉包を加えます。
`stdlib::tests::opt_in_modules_are_reached_only_through_their_names` が std のソースを字句解析・構文解析して、常に読み込むモジュールが opt-in モジュールを名指ししないこと、instance の組み込みクラスが `names` にあること、`@alias` の宣言が `aliases` と一致すること、`uses` が正しいことを検査します。
この選択は、opt-in モジュールの名前を書かないプログラムの型検査の時間（空のプログラムの `check` で約 2 倍になっていた）と IR（関数番号のずれ）を、opt-in モジュールの追加前と同じに保ちます。
関数の由来情報は `CheckedFunction.origin`（`FunctionOrigin`）によって一元管理され、自動生成された `$lambda`、`$task`、`$builtin`、`$case`、`$export` などの補助関数は呼び出し元の `module` や `test` を継承し、`parent` フィールドに親関数の ID を保持します。
LLVM コード生成時、ユーザー由来の関数はすべて出力されますが、std 由来の関数については、ユーザーの通常コード（テスト関数を除く）、エクスポート関数、またはエントリーポイントから参照されて到達可能なもののみが出力対象となります（`reachable_functions`）。例外は native の実行ファイル（`emit_native_build` の `Entry::Console`。`build`・`run`）で、エントリーポイントとエクスポート関数（と、`Drop` の実装。std のものを含む）から到達できる関数だけを出力します（`Instrumentation::executable_roots`。PB03 D1）。実行ファイルでは利用者の公開関数も `internal` で外から呼べないのに、`-O0` ではすべてコンパイルされるためです。`--emit llvm`・`object`・`header`・`wasm`・`shared`、ライブラリの `Entry::Library`、`emit_native_build_for`、`llvm::emit` 系は今までどおり公開関数をすべて出力します。未使用の公開関数も型検査・所有権検査の対象で、その型エラーは `check` が報告します。実行ファイルが出力しない関数だけが使う OS API、定義のない `extern` のシンボル、Net・Async・タスクなどのランタイムは、実行ファイルのリンクにもプラットフォームの検査にも現れません。そのため、以前は失敗したビルドが通ります（`File.read_text` だけを使う未使用の公開関数は、`x86_64-pc-windows-gnu` で `E2002` でした。`extern` のシンボルをどのライブラリも持たない場合は、リンクの失敗でした）。入口か `export def` が使うなら、今までどおり失敗します。名前付きレコードおよび union の型定義は、ユーザー定義の型、および実際に出力される関数のシグネチャや本体から集められたものだけが出力されます。ただし剪定した実行ファイルは、剪定しない場合のプログラムが定義する型をすべて定義します。drop glue と builtin は、出力される本体に現れない型（`Channel.Sender<'a>` の drop が読む `'a`）を読むからで、定義は 1 行で済みます。

`std/Maybe.tc` および `std/Result.tc` は、特別なコンパイラマジックを用いず、通常のジェネリック union と関数のみを用いて型、基本操作、およびビルダーを定義しています。
専用の型付き命令や特殊なランタイムは存在せず、union のタグ分岐、ペイロードの move／clone／drop、および既知の継続に対する特殊化の最適化を一般コードと完全に共有しています。
`get` は網羅的な match の失敗節において `unreachable : unit -> 'a` を呼び出します。
空のブロック本体も成功として反復を継続できるよう、`Zero` 操作は unit の成功値（Maybe では `Some ()`、Result では `Ok ()`）を返します。
スカラー型を扱うビルダーの到達可能な実行経路において、不要な動的メモリ確保や間接関数呼び出しが存在しないことが LLVM IR の検査によって保証されています。
なお、汎用的なアダプタ関数の定義自体にはメモリ確保コードが残り得るため、IR 全文における `@tz.alloc` の有無と、実際の実行ホットパスにおけるメモリ確保の有無を混同しないよう注意が必要です。

### 組み込み関数

`Builtin` は `BuiltinScheme`（型変数、制約、std の型、および `UnsignedOf`／`WidenOf` などの型族）によって型情報を表現し、参照時は具体化された型引数を保持する `FunctionRef::Builtin(BuiltinInstance)` のみを使用します。
モジュール修飾名を持つ組み込み関数（`Task.run` など）は、モジュール関数の解決後、型クラスのメソッド解決よりも前に優先して解決されます。
型族（type family）は引数が具体的な整数型に確定した呼び出し位置で解消され、関数の末尾まで型変数のまま未確定で残った場合は `E1015` エラーとなります。
すべての組み込み関数は scheme に定められた引数個数を持つ `$builtin` ラッパーを経由して呼び出され、部分適用（カリー化）は通常の関数値と同様の仕組みで lowering されます。
LLVM 上の定義は具体化の組み合わせごとに 1 回だけ `@tz.builtin.name` に型引数名を `.` で連結したシンボルとして出力され、型名に含まれる特殊記号は `$A`（`->`）、`$L`／`$R`（`[`／`]`）、`$C`（`,`）などの英数字に置換して LLVM 識別子に適合させます。

#### 引数の順序とコンパイラのフック

`std/` の公開関数と組み込み関数は、操作する値（対象）を最後の引数に取ります（D-59）。`tests/std_argument_order.rs` が、`std/` の全ファイルの `def` 行と、`Array`・`List`・`Vec`・`String`・`Utf8String` の組み込みの scheme を検査し、分類していない新しい `std/` のファイルを拒否します。コンパイラが名前や引数の位置で見ている関数は、順序を変えるときにそのフックを同時に直します。

- `src/llvm_strings.rs` の `compact_body`: `compact` の `String.slice` を、引数の型が `i64`・`i64`・`ref string` の関数として読み（`%p0`・`%p1` が位置、`%p2` が文字列）、`String.join` を `ref string`・`ref [string]` の関数として読みます。型が合わない関数は、言語で書いた本体のままになります（確保が増えるだけで、結果は変わりません）。
- 組み込みの `Utf8String.decode_at`: `src/check.rs` の scheme と `src/llvm_bulk.rs` の生成が、位置（`i64`）、文字列（`ref utf8string`）の順で読みます。
- 組み込みの `Array.__set`・`Array.__sort_with`・`Array.update`・`Array.swap`・`Array.write`・`Array.swap_in`・`Array.split_at_mut`: 対象の配列が最後の引数です。
- `src/derive.rs` が呼ぶ `Json`・`Result`・`Maybe` の関数は、導出コードの生成と対になっているので、順序を変えません。

### エントリー

`Main.tz` のトップレベル式を統合した非公開の合成関数、または `Main.main` の関数 ID をエントリーポイントとして保持します。両者の併用は許可されず、他モジュールや `Main.tc` 内の `main` がエントリーに選ばれることはありません。
`Main.main` のシグネチャは `unit -> i32` または `[string] -> i32`（`Array<string>`）のいずれかのみが認められており、チェッカーがシグネチャ収集時に `check::entry_signature` で検証し、それ以外のシグネチャは `E2004` エラーとして拒否します。
トップレベルの `let` は通常のローカル変数束縛へと lowering され、モジュールレベルの共有可変状態を導入することはありません。
ライブラリ出力モードでは、コンソール用ラッパーやトップレベルコードの自動実行コードは追加されません。
トップレベル式の評価値は、数値、`bool`、文字、文字列であればそのまま標準出力へ表示され、それ以外の型は `Display` インスタンスが存在する場合に `to_string` を経由して表示され、インスタンスがなければ破棄されます（`entry_result`）。
IO の `let!` や `do!` の後に通常の式が続くトップレベルコード（`$implicit.root.$entry`）、およびビルダーを持たない既知の戻り値型（`def main :: unit -> i32` の本体の `i32` など）の本体は、IO の bind 処理を std の非公開関数 `IO.run` で順次直接実行します（direct style。直接実行が許可されているビルダーは IO のみです）。

### 標準入出力

`std/IO.tc` は不透明な `IO<'a>` 型の内部に通常の `unit -> 'a` クロージャを保持します。`pure`、`bind`、`map`、`Delay`、`Combine`、`For`、`While`、`MergeSources` はすべて通常の Tsuzuri ソースコードとして記述されており、独自のタスク機構、ガベージコレクタ、あるいはエフェクトインタープリタなどを新設することはありません。
`IO.__read_line` および `__write` は、std 由来の IO モジュールからのみ参照可能な内部組み込み関数です。低水準の読み込み処理は `(i32 * [ubyte])` のタプルを返し、書き込み処理はステータスコードを返します。`Maybe` や `Result` への変換および UTF デコードは std 側のコードで処理されます。
LLVM は既存の `host_result_slot` および `read_host_result` を再利用して記述子の初期化と検証を行います。IO 専用の外部呼び出しに純粋性属性（pure/readnone）は付与せず、所有バッファの解放には一般コードと同一のアロケータおよび `drop` 機構を使用します。
`IO<T>` のエントリーポイントでは `tsuzuri_main` が生成され、通常のクロージャ ABI（引数 `unit`、環境ポインタ、`borrow=false`）を通じて 1 回だけ消費実行されます。結果の値 `T` は `FunctionEmitter` の既存の `drop_value` によって適切に解放され、再帰型の解放登録も共有されます（ただし、プロセスの終了コードとなる `IO<i32>` の値だけは解放せずにそのまま返却します）。ネイティブ実行ファイルでは C の `main` からこれが呼び出され、オブジェクトファイルや WASM 出力ではホスト側からの明示的な呼び出しを想定し、結果の自動表示処理は付加しません。
ネイティブの `io.c` は、必要性が生じた場合にのみ既存のタスク／CPU ランタイムと同一の C 結合経路へ追加されます。標準 C の `fgetc` による stdio バッファから 1 行ずつ読み込み、幾何級数的に伸長するバッファを通常のアロケータで管理します。`EINTR` によるシグナル中断時は自動で再試行し、末尾の LF／CRLF は適切に除去します。`fwrite` は部分書き込みをループで進行させ、flush の失敗もステータスとして呼び出し元へ通知します。
WASM では、`tsuzuri_io` の同期的な `read_line`／`write` およびメモリ／アロケータを必要時のみエクスポートし、ホスト不在を単なる no-op（無処理）として黙認することはありません。デフォルトの計算専用モジュールに対して不要なインポートを増やすこともありません。
ドライバーの `run` サブコマンドは標準入力・標準出力を子プロセスへ透過的に継承し、標準エラー出力はバッファリングしながらリアルタイムで転送します。JSON 診断モードの場合にのみ標準エラー出力を保持し、プロセス異常終了時の診断情報に含めます。
検証は `tests/io.mjs` により、ネイティブおよび WASM の `-O0`／`-O3`、コールド実行、順序性、EOF、文字コード、障害発生時の動作、ABI 境界、対話的 CLI 実行、オブジェクト出力、ASan／UBSan、ならびにメモリリーク（未解放バイト 0）の検証が行われています。

### OS API（E08）

`File`、`Dir`、`Path`、`Env`、`Time`、`Random`、`Os`、`Process` は通常の std ソース（`std/*.tz`）として実装されており、OS に作用する操作はすべて遅延評価される `IO<Result<_, Os.Error>>` として表現されます（`Path` および `Random.Pcg` は純粋計算です）。コンパイラ本体が提供する低水準境界は、`Os.__read`、`__args`、`__write`、`__random`、`__clock`、`__sleep`、`__open`、`__handle`、`__close`、`__spawn` の 10 個の `Builtin` のみに限定され、新しい型、型クラス、あるいはランタイム関数をコンパイラコアへ追加することはありません。
これらは `IO.__read_line` や `__write` と同様に、std の `File`、`Dir`、`Env`、`Time`、`Random`、`Process`、`Os` 以外のモジュールから直接参照することはできず、ユーザーコードからの参照は `E1022` エラーとなります（`polymorph.rs`）。
第 1 引数に渡される整数によって具体的な操作が選択されます。`__read` は 0: ファイル読み込み、1: ディレクトリ一覧、2: カレントディレクトリ取得（パス引数は無視）、3: 環境変数取得、4/5: メタデータ取得（4 はシンボリックリンクを追跡、5 は追跡しない）。`__write` は 0: 新規作成・切り詰め、1: 新規作成・追記、2: mkdir、3: rmdir、4: unlink。`__open` は 0: Read、1: Write、2: Append、3: CreateNew。`__handle` は 0: read、1: write、2: flush。`__clock` は 0: モノトニック時計、1: Unix エポック時です（単位はナノ秒、取得失敗時は `INT64_MIN`）。
戻り値の状態ステータスは `i64` であり、成功時は 0、失敗時は `(kind << 32) | (errno & 0xffffffff)` となります。エラー種別 `kind` は 1: NotFound、2: PermissionDenied、3: AlreadyExists、4: InvalidInput、5: InvalidEncoding、6: Interrupted、7: Other（`Os.ErrorKind` の定義順）であり、std の `Os.error_of_status` がこれを `Os.Error { kind, code }` 構造体へと復元します（Tsuzuri 自身が検出したエラーの `code` は 0 となります）。LLVM は `status == 0 || (status >> 32) - 1 < 7` の妥当性を検証し、範囲外の値に対しては `read_line` と同様に `BoundsCheck` トラップを発生させます。`__open` は正のファイルハンドル値または負のエラー状態値を返すため、負の場合にのみ符号を反転して検証を行います（`__clock` はステータス検証を行いません）。
バイト列を返すプリミティブ（`__read`、`__args`、`__random`、`__handle`、`__spawn`）は、IO と同一の `host_result_slot`／`read_host_result` 記述子を先頭の出力先ポインタとして受け取り、成否にかかわらず結果を書き込みます（失敗時は NULL ポインタと長さ 0）。LLVM はその記述子と状態値から `(i64 * [ubyte])` のタプルを構築し、所有バッファは通常のアロケータによって安全に `drop` されます。`ref utf8string` はスタック記述子のポインタと長さへ、共有の `ref [ubyte]` は記述子の値そのもの（ポインタと長さ）へと展開して渡されます。

### ネットワーク API（E09）

`Net` は opt-in の std ソースで（D-40 の `stdlib::OPT_IN`）、アドレスの解析と表示は純粋な Tsuzuri、ソケットの操作は遅延評価される `IO<Result<_, Os.Error>>` です。コンパイラ本体の境界は std 専用の `Builtin` 12 個（ブロッキングの `Net.__resolve`・`__open`・`__accept`・`__read`・`__write`・`__close`・`__classify` と、Phase 2 の `__watch`・`__unwatch`・`__connect`・`__names`・`__send`。`polymorph.rs` が std の `Net` 以外からの参照を `E1022` にする）で、`src/llvm_io.rs` の `net_builtin` が `declare i64 @tsuzuri_net_<name>(...)` を到達時だけ宣言します（`readnone`／`readonly` は付けません）。アドレスは scalar（`family << 16 | port`、`high`、`low`）で渡し、runtime から 20 byte の address record（family、0、port、16 byte）で受け取ります。状態の検査は `os_builtin` と同じで（`Net.__open`／`__accept` は正のハンドルか負の状態、`__classify` は 0〜6）、範囲外は `BoundsCheck` のトラップです。
ソケットは `File.Handle` と同じく、runtime の世代検査付きの表（`src/runtime/net.c`、mutex で保護）を指す `Copy` の添字です。世代は開くたびに増え（2^31 で 1 に戻る）、閉じた・古い・種類の違うハンドルは `InvalidInput`（`EBADF`）です。表は開いているソケットが無くなったときに解放し、確保は表だけが C ライブラリの `malloc` を使い、結果の buffer は `tsuzuri_alloc` です。内部ではすべて非ブロッキングで、呼び出しは「システムコールを試し、`EAGAIN` なら `poll` を呼び出し全体の期限（`CLOCK_MONOTONIC`）まで待ち、また試す」形です。受信は 64 KiB 以下ならスタックの buffer に受けて正確な長さへ複写し、それより大きければ結果の buffer に直接受けます。UDP は `recvmsg` で受け、`MSG_TRUNC` を `EMSGSIZE` にします。`accept` の相手のアドレスは `accept` が返したものを使います（macOS では、相手が先にリセットした接続の `getpeername` が失敗するため）。相手が `accept` の前に切った接続は `accept` のエラーにしません（`TZ_NET_GONE_BEFORE_ACCEPT`: `ECONNABORTED`。Windows は `WSAECONNRESET` も。同期版も `accept_async` も同じ `tsuzuri_net_accept` なので、ループは次の接続へ進みます）。Windows の UDP ソケットは、`tz_net_create` が `WSAIoctl(SIO_UDP_CONNRESET)` で、閉じたポートへの送信が次の受信を `WSAECONNRESET` で失敗させる既定を切り（切れなかったときのために、`tz_net_receive_datagram` もその状態を飛ばします）、POSIX と同じにします。どちらの Windows の分岐も、実行は CI だけで確かめます（`tests/net_runtime.c` の `test_accept_reset`・`test_datagram_after_closed_port` は POSIX の契約を固定し、`tests/net.mjs` の async の `accept_reset`・`udp` の `vanished` が Windows の CI で同じ手順を実行します）。
連結は `os.c` の直後（`_DARWIN_C_SOURCE`／`_GNU_SOURCE` は `#ifndef` 付きで同じ値）で、`-pthread`（Windows は `-lws2_32`）を付けます。`tsuzuri test`・`tsuzuri bench` の runner は `net.c` を別の翻訳単位として `os.c` や `io.c` と同じ条件で追加します。既定の wasm32 と `--wasm-host wasi` は `E2000`（`--wasm-feature net` は例外で、下の Phase 3）、native は対応する 5 環境だけです（`Platform::net`）。対象外の native target は引数解析で `E2000`、`--freestanding` でのソケットも `E2000` です。

### 非同期（E09 Phase 2）

B08 の反応器は、タイマーと外部の完了（mailbox と `tsuzuri_async_post`）だけで、kqueue／epoll／IOCP を持ちません。そこで `Net` の非同期操作は、`Async.host` の上に作り、ソケットの準備の監視だけを `net.c` の 1 本の分離スレッドに任せます。スレッドは最初の待ちで始まり、`poll`（Windows は `WSAPoll`）を、起床用の pipe（Windows は自分宛ての UDP ソケット）とすべての待ちに対して呼び、待ちが無くなると終わります。待ちの一覧は 1 つの mutex の下にあり（登録順の通し番号で並ぶ）、`poll` の間は mutex を持ちません。完了（0 か状態）は IR が渡す `tsuzuri_async_post` に入れ、完了は何も所有しません。読み書きと `accept` は、準備のあとに実行器のスレッドがもう一度試します。時間制限 0 は「1 回だけ試す」で、待つ必要があれば `kind` が `Other`・`code` が 0 の状態を返します。接続の途中のソケットだけが待ちの所有物で、取り消し（`Net.__unwatch`）と post の拒否で閉じます。ソケットを閉じるときは、その記述子を閉じる前に待ちを外して起こします。ロックの順序は監視の mutex、ソケット表の mutex です（逆順に取る経路は無く、`tsuzuri_async_post` は監視の mutex の下で呼びます）。`Async.tc` は変えていません。`Async.all_results` が捨てた、完了済みで届いていない操作には `cancel` が呼ばれないので、完了が何も所有しないことが資源の安全性を支えます。コンパイラは、`@tsuzuri_net_watch` か `@tsuzuri_net_connect` を参照するのに `Async.block_on` に到達しない native のビルドを `E2000` にします。

### `Net.resolve` とアドレスの文字列（E09 のレビュー対応）

アドレスの文字列を読むのは `std/Net.tz` の厳密な解析器（`parse_ip`）だけで、ランタイムも OS も読みません。`resolve_with` は、host が `:` か `%`、空白か制御文字（0x20 以下と 0x7f）、ASCII 以外のバイトを含むか、最後のラベル（末尾の `.` を除く）が数字だけか `0x`・`0X` で始まるとき（`numeric_looking`）、`Net.__resolve` を呼ばずに `parse_ip` の結果（1 つのアドレス、または `InvalidInput`）を返します。`getaddrinfo` は `127.1`・`0x7f000001`・`2130706433`・`1.2.3`・`fe80::1%lo0` のような `inet_aton` の形やゾーン付きの書き方を数値のアドレスとして読み、読み方が OS で違う（`0177.0.0.1` は macOS が 10 進数）ので、そのまま渡すと `parse_ip` で名前と分けた検査を抜けられるためです。ASCII 以外のバイトは、Node.js の `dns.lookup` が IDNA で ASCII に直してから OS に渡し、全角の `１２７．０．０．１`・`127。0。0。1`・`127｡0｡0｡1`、`127.0.0.1` の後ろの U+00AD・U+200B などが 127.0.0.1 に引けてしまうので（wasm32 のホストでの 2 回目のレビュー対応）、全ターゲットで同じに断ります。`tsuzuri_net_resolve` はさらに、`AI_NUMERICHOST` の `getaddrinfo` を先に 1 回呼び、成功したら（システムが数値と読む host が分類を抜けたとき）`InvalidInput`（`code` 0）にして名前の問い合わせへ進みません。検証は、`tests/fixtures/net_resolve` の約 880 個の host（手書きと seed 付きの生成）を `tests/net.mjs` の基準（Node の `net.isIPv4`／`isIPv6` と URL）と比べる T9b、`tests/net_api.rs`、`AI_NUMERICHOST` を基準にした網羅でランタイムの拒否を直接見る `tests/net_runtime.c`、`getaddrinfo` の呼び出しを数える確保の追跡の harness（システムが数値と答えた回数が 0、名前の `localhost` だけが 2 回）です。

検証は `tests/net_api.rs`（17 件）と `tests/net.mjs`（アドレスの 3,200 入力を Node と照合、`resolve` に渡すアドレス文字列 894 個、ソケットの全 case、非同期の case、ASan／UBSan と確保の追跡、`TSUZURI_TSAN=1` の ThreadSanitizer）、`tests/net_runtime.c`（コンパイラなしの監視スレッドと、システムが数値と読む host の拒否の検査）です。Windows は `tests/windows.rs` が MSVC の SDK で x86_64 をリンク、aarch64 をコンパイルし（実行は CI だけ）、`.github/workflows/vscode.yml` が Net の手順を最後に持ちます。

### wasm32（E09 Phase 3）

既定の wasm32 は import を持たず、WASI preview1 には `connect`・`bind`・`listen` がなく、preview 2 のソケットはコンポーネントモデルを要するので、ソケットを持つのは `--wasm-feature net`（`--wasm-feature jspi` が必要）を明示した Node.js 向けのビルドだけです（`driver::BuildOptions::validate` が、wasm32 の object・LLVM IR・WASM・bindings-js 以外、`jspi` なし、threads や `--wasm-host` との併用を `E2000` にします）。IR は native と同じ `tsuzuri_net_*` の宣言のままで（`net_async_builtin` は wasm では `__watch`・`__connect` から `tsuzuri_async_post` の引数を除きます）、`llvm::with_net_imports` が、宣言された（到達した）ものだけの `declare` に `"wasm-import-module"="tsuzuri_net"` と `"wasm-import-name"` を付けます。`net.c` は連結せず、wasm-ld は `tsuzuri_alloc`・`tsuzuri_free` とメモリを export します（結果の buffer をグルーが書くため）。グルーは `src/bindings.rs` の `javascript_with_net` が作り、ドライバーは、ソケットに到達するモジュール（`llvm::reaches_net`。`reachable_functions` の本体に `Net.__*` の組み込み関数があるか。公開関数が呼ぶ分も、IR に `@tsuzuri_net_` が出るのと同じに数えます）にだけそれを使います。到達しないモジュールは、`.wasm` が `tsuzuri_alloc` を export しないので、`jspi` だけのグルーになります（グルーの表の `hostAbi` が alloc・free・memory の export を要求するため）。表に `net: true` と `hostAbi: true` を足し、`bindings-net-host.mjs`（Node.js の `node:net`・`node:dgram`・`node:dns`・`node:os` を top-level await で読み、読めなければ `NODE_NET` が空で、`netImports` が失敗する）を前置し、`bindings-net.mjs` を `bindings-core.mjs` の 3 か所（`inspect` が許す import 名、`bind` の import の組み立て、`tsuzuri_net` の登録）へ差し込みます。差し込み先は `splice` が一意であることを assert し、ソケットに到達しないグルーは `--wasm-feature net` の有無で変わりません。`bindings-net.mjs` は `net.c` の契約（世代検査付きのハンドルの表、`(kind << 32) | code` の状態、呼び出し全体の期限、時間制限 0 の「1 回だけ試す」と `PENDING`）の JavaScript 版で、待つ 5 個（`resolve`・`open`・`accept`・`read`・`write`）が `WebAssembly.Suspending`、時間制限 0 の `accept`・`read` は、`.wasm` が実行時に分ける普通の import `try_accept`・`try_read`（`llvm_io.rs` の `waiting_call`。wasm のときだけの分岐と `phi`）です。`Async.start` の実行器はホストが `WebAssembly.promising` の外から進めるので、中断する import を呼べず、Node.js 24 の V8 は import が同期の値を返しても中断を試みて `SuspendError` にするためです。結果の buffer はモジュールのメモリへ複写して記述子（ポインタと長さ）を渡し（ポインタは `>>> 0` で符号なしにします）、`watch`・`connect` の完了は呼び出しが戻ったあとに `setImmediate` で `tsuzuri_async_complete` を呼びます。import の本体は `guard`・`guardAsync` で包み、Node.js が投げた例外（ポート 0 の `dgram.send`、資源の枯渇）をその呼び出しの状態にします（モジュール自身の trap だけは通します）。ソケットは、モジュールが待っているあいだだけ `ref` して（`retain`・`release`）、待っていないものはプロセスを生かさず、`fail` がインスタンスを捨てるときに `owner.dispose` がすべて閉じます。読まれないまま届く分は、UDP が 1 MiB・4096 個、listener が 128 接続までで、あふれた分は捨てるか RST で切ります。UDP の非同期の送信は、結果を Node.js が次のターンに返します。`__send` はその呼び出しの中で `socket.send` を 1 回だけ呼び、`-(ACCEPTED + token)`（`ACCEPTED` は kind 7 の code 2^24 で、どの errno よりも大きく、kind はコンパイラの状態の検査を通ります）を返します。std の `send_loop` は、この値を見たら `__watch` を `events = 2 + 4 * token` で呼び、グルーの `watchSend` が、その送信の `dgram` のコールバックが渡した状態（0 かその送信の失敗）で操作を完了します。送信は 1 回しか行わず（呼び出しをやり直しません。std が渡す配列は呼び出しごとに複製されるので、ポインタでやり直しを見分けることはできません）、結果はその送信のものです。取り消された待ちの送信は、終わったときに表から消えます。ソケットの `close` は、`inflight`（Node.js が終えていない送信）が 0 でなければ、ハンドルを無効にしたまま、最後の送信のコールバックで閉じます（閉じたソケットは、まだ出ていない送信を捨てるためです）。受信側には送信の失敗を残しません。accept した接続の名前は、`connection` の event で記録します（`socket.remoteAddress` は `getpeername` を遅れて呼ぶので、切られたあとでは答えません）。エラー番号の表は、`os.constants.errno` に、libuv の表（`util.getSystemErrorMap()`）にだけある名前（`EHOSTDOWN`）を足したものです。TCP は `allowHalfOpen` で、相手の半クローズがこちらの書き込みを終わらせません。接続が読まれないまま RST で終わったときは、届いたデータのあとに 1 回だけエラーを返してから EOF にします。検証は `tests/net_wasm.mjs`（Node.js 24 の JSPI、loopback。上のそれぞれの振る舞いと `Async.start` の `_async`、2 GiB を超えるアドレス）と、`tests/net_api.rs` の import とグルーの検査、`tests/bindings.mjs` の import 名の走査（`memory64` の読み違いの回帰）です。

### OS API のリンクと入口

到達可能性解析によって実際に到達したプリミティブのみを `declare i64 @tsuzuri_os_<name>(...)` として抽出し、`BTreeSet` で管理される intrinsic 集合に追加するため、外部宣言の重複は発生せず出力は完全に決定的となります。OS API を使用しないプログラムでは、生成される IR やランタイムの構造は一切影響を受けません。`Os.__args`（`Env.args`）に到達した場合にのみ `declare void @tsuzuri_os_set_args(i32, ptr)` も追加宣言され、この宣言が存在するときに限り、C のエントリーラッパーが `@main(i32 %argc, ptr %argv)` となり、`tsuzuri_main` の呼び出しに先立って argc と argv が保存されます（なお、`Env.args` に argv[0] は含まれません）。それ以外のプログラムの入口は従来の引数なし `@main()` のままです。`@tsuzuri_os_` を含む IR に対しても、アロケータは `tsuzuri_alloc` および `tsuzuri_free` を適切に公開します。
ネイティブの `src/runtime/os.c` は、IR が `declare i64 @tsuzuri_os_` を含んでいる場合にのみ、ドライバーによって C ランタイムの単一コンパイル単位（translation unit）へと組み込まれます。連結順序は `os.c`、`trap.c`、タスクランタイム、`cpu.c`、`io.c`、`arguments.c` の順であり、`os.c` を先頭に配置するのは `_DARWIN_C_SOURCE` や `_GNU_SOURCE` を先頭の `#include` よりも前に定義する必要があるためです。公開される関数は weak かつ hidden 属性を持ちます。
`IO<i32>` を返すトップレベルエントリーポイントでは、戻り値の整数を解放（drop）せずに `tsuzuri_main` の戻り値、すなわちプロセスの終了コードとして直接返却します。`llvm::exit_code_entry` が `def main` と std の `IO<i32>` の組み合わせを判定し、それ以外の `IO<T>` では結果を drop して正常終了コード 0 を返します（Tsuzuri に `Os.exit` 関数は存在しません）。`tsuzuri run` は、`exit_code_entry` を持つネイティブプログラムが非ゼロで終了した際、トラップエラーとは明確に区別して `E2005`（`program exited with code N`）を報告します（JSON 診断モードでは標準エラー出力の内容も添付されます）。

### `def main` の入口

`llvm::main_entry` によるエントリーポイントでは、コンソール用の `@main` が `Main.main` を `i8 0`（`unit`）または `%tz.array` の引数で呼び出し、戻り値の `i32` をそのままプロセスの終了コードとして返します（コンソールへの値の表示は行いません）。
`Array<string>` を受け取る `main` 関数では、ラッパー関数が `@main(i32 %argc, ptr %argv)` として定義され、`declare void @tsuzuri_arguments(i32, ptr, ptr)` によって生成された `%tz.array` を所有権ごと渡します（呼び出された callee 側が配列を drop します）。もし `Env.args` にも到達している場合は、それに先立って `tsuzuri_os_set_args` を呼び出します。
ドライバーは、IR 内に `@tsuzuri_arguments(` が含まれている場合にのみ `src/runtime/arguments.c` を C ランタイムのコンパイル単位の末尾に追加し、IR 側は公開アロケータ（`tsuzuri_alloc`／`free`）を提供します。各引数は `tsuzuri_alloc` によって確保された UTF-16 の文字列（`%tz.string`）となります。
POSIX 環境では argv[1..] の UTF-8 文字列をデコードして格納し（不正なバイトシーケンスは U+FFFD に置換されます）、Windows 環境では `GetCommandLineW` から取得した文字列を空白およびタブ文字で分割します。ダブルクォーテーションで囲まれた範囲（例: `"b c"`）は内部の空白で分割せずに 1 つの引数として扱い、引用符自身は除去され、バックスラッシュは特別扱いされません。なお、先頭の実行プログラム名（argv[0]）は破棄されます。
ライブラリ出力モードでは `tsuzuri_main()` が定義され、ネイティブでは空の引数配列が渡されます。WASM 出力では IR 内の弱いシンボルである `tsuzuri_arguments`（空配列を返す）が呼び出され、`--wasm-host wasi` 指定時には `os-wasi.c` の後に `arguments.c` を結合した強いシンボル定義が `args_sizes_get` および `args_get` を通じて実際の引数を取得します。
`exit_code_entry` は `def main` を含むため、`tsuzuri run` における `E2005` の報告や WASI の `proc_exit` 呼び出しは `IO<i32>` のエントリーポイントと同様に機能します。`tests/arguments.rs` と `tests/arguments_runtime.c` が引数の分割・デコードと実行ファイルへの受け渡しを検証し、`tests/os.mjs` がネイティブおよび WASI での引数処理を検証しています。

### OS API と wasm・Windows

WASM のデフォルト出力では、`tsuzuri_io` 以外のホストインポートを持たない独立バイナリの生成を原則としています。そのため、OS プリミティブに到達した WASM オブジェクト、LLVM IR、およびスタンドアロン WASM の出力は、ファイル生成前に `E2000`（`OS_WASM_MESSAGE`）エラーとして拒否されます（ただし、`Path` の操作、`Os` 内の純粋な補助関数、および `Random.Pcg` はプリミティブを呼び出さないため、インポートなしで安全にビルド可能です）。また、Windows 上でコンパイラを動作させてネイティブの実行ファイルやオブジェクトを生成する際、OS プリミティブに到達している場合は `E2002`（`OS_WINDOWS_MESSAGE`、`--emit llvm` を除く）エラーとなります（`src/runtime/os.c` に Windows 向けの実装がないためです。[G10](../_features/_completed/G10-windows.md) は Windows の実行の確認で、この実装を含みません）。
`--wasm-host wasi`（`BuildOptions.wasm_host`。ビルド時のみ指定可能）は wasm32 のオブジェクトおよびスタンドアロン WASM 出力でのみ指定可能であり、`--wasm-feature threads` や `--emit llvm` との併用は拒否されます（`E2000`）。これは、`os-wasi.c` をオブジェクトとしてコンパイルして静的リンクする構成をとっているため、IR のみを出力するモードでは `tsuzuri_os_*` や `tsuzuri_io_*` の未解決シンボルが残存してしまうためです。`llvm::with_wasi_host` は標準 IO の宣言から `tsuzuri_io` のインポート属性を除去し、`src/runtime/os-wasi.c`（freestanding C11）を別オブジェクトとしてコンパイルして結合します。`os-wasi.c` は `os.c` や `io.c` と同一のインターフェイスを `wasi_snapshot_preview1` 上に実装しており、到達した関数に対応する WASI インポートのみが追加されます。ファイルパスは preopen されたディレクトリのうち `/` 境界で最長一致するプレフィックスへ解決され、いずれにも一致しない場合は最初の preopen ディレクトリからの相対パスとして扱われます。`Os.Error.code` には WASI の errno が格納され、`Env.current_dir` は最初の preopen ディレクトリ名となり、`Process.run` は `Other`（未対応）エラーとなります。`_start` は `--emit wasm` 指定時、かつエントリーポイントが `def main` または IO である場合にのみ定義され（`-DTZ_WASI_START`）、`def main` や `IO<i32>` の非ゼロの戻り値は `proc_exit` へ引き渡されます（`-DTZ_WASI_EXIT_CODE`）。オブジェクト出力時の `_start` の定義はホスト側に委ねられます。なお、WASI preview2 および WebAssembly コンポーネントモデルは現時点で未実装です。

### ファイルハンドルと子プロセス

`File.Handle { id: i64 }` は、ランタイム内部のファイルテーブルのエントリを指す不透明な Copy 値であり、あえて `Drop` を実装していません（標準ライブラリの型は言語規則上 `Drop` を勝手に実装できず（`E1016`）、また `Drop` を持つ値は `let!` の継続境界を越えられないという言語仕様（`E1005`）が存在するためです）。ハンドルの内部値は `(generation << 32) | (slot + 1)` としてエンコードされ、ファイルをオープンするたびに一意で再利用されない世代番号（generation）がインクリメントして割り当てられます（世代が $2^{31}-1$ 回に達した場合は `EOVERFLOW` 起因の `Other` エラーとなります）。クローズ済みまたは古い世代のハンドルを用いてアクセスした場合、同じスロット記述子番号を再利用している別のファイルへ誤ってアクセスすることはなく、確実に `InvalidInput`（EBADF）エラーとなります。ファイルテーブルは必要に応じて倍々に拡張され、すべてのオープンファイルがクローズされると自動的にメモリが解放されます。`File.with_open` を使用すれば、例外や早期リターンを含むすべての実行経路で確実にファイルがクローズされます。ファイルは常に `O_CLOEXEC` フラグ付きで安全に開かれ、ディレクトリをファイルとして開こうとした場合は `InvalidInput`（EISDIR）で即座に拒否されます。
`Process.run` は、シェルの介在を挟まずに `posix_spawnp` を直接用いて子プロセスを安全に起動します。引数列は NUL 文字区切りで渡され、子プロセスの argv の各要素へと確実に展開されます。標準入力データはパイプ経由で書き込まれた後に速やかに閉じられ、標準出力および標準エラー出力は `poll` と非ブロッキング読み取りを組み合わせて並行して回収されます。出力データの合計サイズが $2^{30}$ バイト（1 GiB）を超過した場合は、リソース枯渇を防ぐため子プロセスへ `SIGKILL` を送信して強制終了させ、`Other` エラーを返します。子プロセスへの書き込み中に発生した `SIGPIPE` シグナルは、その関数呼び出しの実行中のみ安全に無視されます。

### OS API の検証

`tests/os_api.rs` では、予約名、非公開プリミティブの不可視性（`E1022`）、型整合性、到達した関数のみが外部宣言・ランタイムに含まれること、argv を受け取るエントリーポイントの挙動、`IO<i32>` の終了ステータス、ならびに `Random.Pcg` と `File.Handle` の不透明性を包括的に検証します。`tests/os.mjs` では、ネイティブおよび WASI（`tests/os-wasi-host.mjs` を通じて `node:wasi` 上で実行）の双方を `-O0` および `-O3` で動作させ、実際のファイルシステム、環境変数、コマンドライン引数、終了ステータス、ならびに `E2000`／`E2005` 診断の発生条件を Node.js の `fs` および `os` モジュールの期待動作と厳密に照合します。ネイティブと WASI の実行結果は、OS 固有の errno 番号の差異を除いて完全に一致します。またメモリ追跡ハーネスを用い、生成 IR 内の `@malloc`／`@realloc`／`@free` および `os.c` 内部での libc 呼び出しをインターセプトして計数し、ASan／UBSan 有効環境で 64 回の連続反復実行を行っても、生存メモリブロック数が完全に 0 に復帰することを検証しています。なお、OS API の処理速度については特定の性能主張を行っていません。

### 定数

`Program.constants` で定義された定数は、内部的な引数なし関数宣言として収集され、通常の関数と完全に同一の名前解決、可視性判定、および型検査のルールが適用されます。
特殊化や所有権検査に先立って、明示的なスタックを用いた依存解析が行われ、定数の評価結果はキャッシュされて参照箇所へと型付きリテラルとしてインライン展開されます。
型検査時には条件分岐の双方が通常の静的型検査を受け、定数評価器のみが短絡評価を行います。2進（binary）浮動小数点は `rustc_apfloat`（APFloat）を用いてビット幅ごとに正確に計算され、decimal 演算のコンパイル時評価は拒否されます。
定数の内部宣言は非公開とされ、出力コードの到達性ルートからは除外されます。コンパイラコアに独立した定数ランタイムや共有所有メモリ領域を新設することはせず、既存の文字列リテラル用グローバル領域、集約値の生成機構、およびフレーム管理／ヒープ移送／デストラクタ（frame/relocate/drop）の仕組みをそのまま共有します。
定数や式の一時値（関数呼び出しやパイプライン演算の結果など）を共有借用（`ref`）の引数へ渡す際は、`constants::temporary_borrows` がそれらを `BorrowOperand` に変換して一時値を生成・呼び出し後に安全に解放し、型検査によって「単一段階の完全適用であること」および「戻り値に借用が漏洩しないこと」を保証します。呼び出しを検査する時点で戻り値の型がまだ決まっていない場合（`Array.sort >> Array.rev` の合成は、要素の型が決まる前に `Array.rev` へ `Array.sort` の結果の配列を渡します）は、借用を `BorrowOperand` にせずに残し、関数の型が確定した `Checker::finish` で `settle_temporary_borrows` が同じ規則で決めます。確定した型でも規則を満たさない借用（`Array.sort >> Array.minr` のように、一時の配列を指す参照を返す呼び出し）は、これまでどおり所有権検査が `E1013` で拒否します。
### `const def`

`def` 署名の `const` は構文木の `FunctionDecl::constant`／`SignatureDecl::constant` に保持され（フロントエンドキャッシュの形式は 3）、`Names::const_functions` に集められます。`constants::fold` は、まず全 `const def` の署名と本体を `const_rules` で検査します。署名は値型だけを許し、本体は `TypedExprKind` の全 variant を網羅した `match`（`_ =>` を持たない）で、評価するか `E1026` で拒否するかを決めます。評価は `const_machine::Machine` が行い、`const` の初期化式に現れる `const def` の呼び出し（引数は評価済みの値）もこの評価器へ渡します。
`Machine` は Rust の再帰を使いません。未処理の仕事（`Task`）、中間値（`Value`）、呼び出し（`Frame`、ローカルの束縛）が別々の明示的なスタックにあり、`while`・`for`・再帰呼び出しの深さがコンパイラの stack を消費しません。予算は定数の評価と共有する 1 つの `remaining`（1,048,576）で、式 1 個ごとと値の複製ごと（その値のノード数）に引きます。呼び出しの深さと値の入れ子（`aggregate`）は `MAX_NESTING`（128）までです。整数の幅・丸め・飽和・短絡は定数の初期化式と同じ `unary`／`binary`／`cast` を使い、`match` の arm の選び方と範囲 `for` の終わり方は `llvm_control.rs` の実装（`range_loop`）と同じです。評価前の IR には `GenericInteger`／`GenericFloat` が残るので、`Checker::integer_literal` と `numeric::float_literal` で型に合わせてから使います。
`const def` は実行時にも呼べる通常の関数で、型検査・所有権検査・コード生成は `const` のない関数と同じです。評価結果の値は、他の定数と同じく、値として使う参照箇所へ型付きリテラルとして展開されます。
### 静的データ（D11 Phase 2）

値が平たいデータだけの定数は、読むだけの使い方と借用で、値を展開せずバイナリの 1 つの実体を指します。`constants::statics::eligible` が型で判定し（整数・浮動小数点・decimal・`bool`・`char`・`unit`、それらのタプル・レコード・固定長配列、payload を持たないか全 case が同じ payload 型の union、定数全体としての配列 `[T]`。`needs_drop` な型や利用者の `Drop`、`string`・リスト・`Vec` は対象外）、大きさには依りません。`temporary_borrows` は該当する定数の `ref` 引数を `BorrowOperand` にするだけで、E1013 の判定をしません。`fold` は全定数の評価のあとで、各関数の本体を `statics::mark_uses` で書き換えます。`Borrow(C, false)` と `BorrowOperand(C)` は `Borrow(Static(C), false)`、`C[i]`・`C.length`・`C.field`・共有スライス・`for x in C` の定数は `*(ref Static(C))` を読む形になり、残りの値としての使い方だけを `inline` が従来どおり複製します。`Static(id)` は `Borrow` の被演算子にしか現れない葉で、`CheckedModule::statics`（定数の宣言の番号から `StaticData`）が値のリテラル木を持ちます。評価は書き換えの前に終わるので、`Machine` が `Static` を見ることはありません。静的データの合計は `MAX_STATIC_BYTES`（4 MiB）までで、超過は上限を越えた定数の宣言への `E1026` です。
所有権検査は `Static(id)` を、ローカルの番号とも引数の根（`usize::MAX - id`）とも重ならない根 `STATIC_ROOT + 1 + id` の場所として扱います。静的データは書き込みも move もできず、その根への借用は読み取りだけなので互いに衝突しません。`check_result` と `store_through` は静的な根の借用を許し（タスクの結果を除く）、借用を返す関数の呼び出しで引数が借用を貸していない結果には、根 `STATIC_ROOT` の借用を 1 つ足します（返せる借用は引数か静的データのものだけだからです）。`Copy`・`Send`・`Sync` の判定と、ほかの `E1013`／`E1014` の規則は変わりません。
生成では、`Borrow(Static(配列))` は `%tz.array` の定数 `{ ptr @tz.static.N.data, i64 長さ }` 自体が借用の値で（空の配列は `zeroinitializer`）、ヘッダーのグローバルも読み込みもありません。配列以外の値は `@tz.static.N` のアドレスです。どちらも `private unnamed_addr constant` で、`Globals::statics`（`BTreeSet`）が定数ごとに 1 度だけ定義します。借用は drop も `free` も `realloc` もされません。値としての読み出しは `clone_value`（所有する複製）か、配列の反復のように複製せず読むかのどちらかで、静的データを move する経路はありません。静的データを使わないプログラムの IR は変わりません。
動作は `cargo test --locked --test constants` および `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri constants`・`node tests/features.mjs target/release/tsuzuri const_functions`・`node tests/features.mjs target/release/tsuzuri const_statics` で検証されています。

### デバッグ情報

`llvm::emit_with_debug_info` は既存の `TrapSource` ソースマップを明示的に受け取り、DWARF デバッグ情報を付与したコードを生成します（通常の API 呼び出しではデバッグ情報は生成されません）。
`DIFile`、`DICompileUnit`、`DISubprogram`、`DILocation`、ならびに変数や型のメタデータノードは `Globals.next_metadata` によって一元的に採番され、ループメタデータ、トラップマーカー、および同梱ランタイムと同一の ID 空間を共有します。
ソース位置情報は既存の `current_span` を利用し、`switch` の case 行ではなく各命令の終端位置に正確に関連付けられます。ローカル変数のエントリーブロックにおける `alloca` の直後には `llvm.dbg.declare` が配置されます。
公開 ABI やコンソール用ラッパー関数にも適切なスコープ情報が付与され、`-O3` 最適化によって内部関数がインライン展開された場合でも元のソース位置情報が確実に保持されます。

DWARF の形は、formatter を読み込まないデバッガーでも Tsuzuri の名前が見え、`scripts/lldb/tsuzuri_lldb.py` が値を復元できるように決めています（G16）。命令列は変えず、`-g` なしの IR は同一です。

- 型名は `Type::display`（`i64`、`[|i64|]`、`Maybe<i64>`、`Main.Shape`）です。`bool` 以外のスカラーは同名の基本型への `DW_TAG_typedef` です（デバッガーは基本型には C の名前を、typedef には typedef の名前を表示するため）。`char`・`utf8char` の基本型は 16・32 bit の `DW_ATE_UTF`、2進浮動小数点は `DW_ATE_float`、decimal 型は BID のストレージとして `DW_ATE_unsigned` です。名前の付くポインター（参照、ハンドル、`Rc`・`Arc`、再帰 union）も、名前のないポインターへの typedef です。
- リストの `head` は chunk 構造体 `<型名>.chunk`（`next`・`rest`・`capacity` と、要素型の長さ 0 の配列 `slots`。`src/llvm_list.rs` の `{ ptr, i64, i64, [0 x T] }`）へのポインターです。header は 24 bytes で、`slots` の offset は header を要素のアライメントに切り上げた位置です。formatter は `head` から `next` を辿り、chunk ごとに最後の `length - rest` 個の slot を読みます。関数値と Task は `descriptor`（構造体 `closure.descriptor` へのポインター。`code` は関数型へのポインター、`clone`・`drop` は型のないポインター）と `environment`（型のないポインター）の 2 メンバー、dyn 値の `data`・`vtable` は型のないポインターです。
- record は構造体で、メンバーは宣言の順に並びます。メンバーの offset は `physical_fields` が返す格納の順で畳んだ位置です（アライメントの大きい順に格納する record では、宣言の順と offset の順が違います）。`tests/debugger.mjs` が LLDB で並べ替えた record の値を読み、DWARF の offset が実際のメモリと合うことを確かめます。
- union はすべての case が値を持たなければ `DW_TAG_enumeration_type` です。そうでなければ構造体で、メンバー `$tag`（列挙型 `<型名>.$tag`）と `$payload`（値を持つ case ごとに case 名のメンバーを持つ `DW_TAG_union_type` `<型名>.$payload`）を持ちます。`$payload` の offset は、`union_layout` が `Common(T)` なら（タグの幅を `T` の align に切り上げた値）、`General { unit, units }` なら（タグの幅を `unit` に切り上げた値）です。メンバー `$tag` と列挙型 `<型名>.$tag` の大きさはタグの格納幅（1・2・4 バイト）で、再帰 union のノードだけは `i32`（4 バイト）です。再帰 union はノード構造体 `<型名>.node`（`next`・`drop`・`clone`・`$tag`・`$payload`。`emit_program` の `{ ptr, ptr, ptr, i32, payload }`）へのポインターの typedef で、空ポインターは最初の値を持たない case です。タグを持たない union（`Niche`。`Maybe<string>` など）は、構造体 `<型名>` が `$payload`（offset 0、ペイロードの case をメンバーとする `DW_TAG_union_type`）と `$niche`（長さの位置にある 64 bit の列挙型 `<型名>.$niche`。関数値・`Task` のペイロードではポインター幅の列挙型）を持ち、列挙子はペイロードなしの case を `-1`（負の長さ）として 1 つだけ持ちます。`$niche` の値がその列挙子と等しければその case で、そうでなければペイロードの case です（`$tag` は持ちません）。`$` は Tsuzuri の識別子に使えないので、record のフィールドと衝突しません。
- `DISubprogram` には `linkageName` がなく、`name` は `CheckedFunction::qualified_name` から単相化の `.$mono.<N>` を除いた名前（`Main.show`、全実体が `Array.sum`）です。ラムダ式と Task は `<外側の関数>.lambda@<行>:<列>`・`.task@<行>:<列>`、callback の特殊化は元の関数と同じ名前です。`export` の `@tz_<name>`、`@tsuzuri_main`、`@main` などのラッパーは、シンボル名と `DIFlagArtificial` を持ちます。組み込み関数・組み込みメソッド・case のコンストラクター・export の bridge（モジュール `$builtin`・`$intrinsic`・`$case`・`$export`）は自分のソースを持たないので、`DISubprogram` を付けません。
- `let` の束縛の `store` は局所変数の宣言の位置に、閉包の捕捉の読み出しは閉包の位置に置きます。引数の `store` は位置を持たず、`DISubprogram` は `scopeLine` を持たないので、`loop` ブロックで引数を束縛するまでの prologue は行 0 になり、デバッガーは関数の本体の最初の行で止まります。
- ランタイムの C（`task.c` など）はデバッグ情報なしでコンパイルし、生成する補助関数（`tz.apply.*`、`tz.drop.rec.*`、`tz.clone.*`）にも位置を付けません。LLDB は既定でデバッグ情報のない関数へステップインしないので、ステップ実行は Tsuzuri のソースだけを辿ります。プログラムの DWARF はコンパイラーの compile unit（DWARF 4）だけになり、Clang の既定の DWARF 5 の runtime の unit を `llvm-link` で結合したときにプログラム全体が DWARF 5 になって、LLVM 21 の `-O3` の `.debug_names` が `llvm-dwarfdump --verify` を通らなかった問題もなくなります。
- Windows で MSVC のリンカーを使うとき（`driver::msvc_linker`。配布物の `tsuzuri-clang` は MinGW の `ld.lld` でリンクするので除く）、driver は `-g` のネイティブの IR に `llvm::with_codeview` で `CodeView` の module flag を足し、LLVM は DWARF と並べて CodeView（`.debug$S`・`.debug$T`）を出します。実行ファイルと `tsuzuri test -g` のランナーのリンクには `-Xlinker` で `/PDB:<一時ディレクトリ>`、`/PDBALTPATH:<出力名>.pdb`、`/NATVIS:`（`src/runtime/tsuzuri.natvis`）を渡し、PDB を `<出力の拡張子を .pdb にした名前>` の sidecar として公開します。`lld-link` では PDB と DWARF の両方が残ることを確かめています（G16 Phase 3）。
- formatter の正本は `scripts/lldb/tsuzuri_lldb.py` で、LLDB の `lldb` モジュールだけを使い、型名と `$tag`・`$payload` のメンバー名で型を見分けます。`scripts/toolchain/bundle.mjs` が配布物の `share/lldb/` へ、`vsc/scripts/toolchain.mjs` が VS Code 拡張の `resources/lldb/` へ複製し、拡張はデバッグの `initCommands` の先頭で `command script import` します。

Clang には `-g` フラグが渡され、WASM リンク時の `--strip-all` は解除されます。macOS におけるデバッグタスクオブジェクトの生成では、対応する `llvm-link` を用いて事前に IR を結合してから単一オブジェクトとして生成することで、`ld -r` による DWARF 情報の脱落問題を回避しています。
また macOS のデバッグ実行ファイルは、保持された `module.o` からリンクを行い、一時ファイルを削除する前に `dsymutil --flat` を実行して隣接する `output.dwarf` を生成します。
DWARF メタデータとトラップ情報テーブルは、共通のソースファイル保護、バックアップ、および公開失敗時のロールバック機構を利用して安全に出力されます。
なお、Cargo の release プロファイルにおける strip 設定はコンパイラ自身のバイナリにのみ適用され、生成対象のバイナリには干渉しません。検証は `tests/debug_info.rs`（名前、束縛の位置、スカラーとポインター、union の形と offset、テストランナーの `tsuzuri_test_run` の subprogram）、`tests/debug_info.mjs` によるネイティブおよび WASM の `-O0`／`-O3` テストと `llvm-dwarfdump --verify`、ならびに LLDB の batch で表示・名前・ステップ実行と `-O2` のテストランナーのブレークポイントを確かめる `tests/debugger.mjs`（LLDB が必要なので共有 CI では実行しません）、プロセスを起動せずに関数値と `Task` の型の形と formatter の `is_function` の判定を確かめる `tests/debugger_static.mjs`（これも LLDB が必要）、LLDB の値の代役でリストの chunk のたどり方（先頭側の空き slot、短い長さの view、alignment 16 の要素の offset、chunk の二分探索、壊れたメモリ）を確かめる `tests/lldb_lists.py`（python3 だけで動き、`tests/debugger.mjs` も最初に実行します）によって行われています。

### MCP サーバー（G29、D-60）

`tsuzuri mcp` は 3 つのスレッドで動きます。読み手のスレッドが標準入力を 1 行ずつ（4 MiB まで）読んで `sync_channel(8)` で渡し、呼び出したスレッドが唯一の書き手として、`initialize`・`ping`・`tools/list`・資源の一覧・取り消しにすぐ答え、`tools/call` と `resources/read` を待ち行列（64 件まで）に積みます。作業のスレッド（`WORKER_STACK_BYTES`、8 MiB）が 1 件ずつ来た順に `Backend::call` を `catch_unwind` の中で実行し、取り消された答えは捨てます。`ProjectBackend` は読み取り専用です: `tz_check` は `Project::load`・`analyze_all`（frontend のキャッシュを書かない）、知識グラフは `target/tz/kg` が新しければ `KgData::open_fresh`（読んだ公開の manifest も新しいこと）と `KgData::read_all` で全部を読んで manifest と照合し（サーバーが持つグラフが後でディレクトリに左右されない。照合できなければメモリの上に作る。`tz_impact` の取り込んだグラフは照合できなければ誤り）、古ければ `kg::build::memory` がメモリの上に作り、std の知識グラフも `StdSource::Memory` です。答えは、ツール名・引数・入力のハッシュ（`tz_check`・`tz_fix_plan` は `path` のプロジェクトのもの）の SHA-256 を鍵に、直近 8 件を再利用します。入れるときの鍵は、ツールが自分で読み込んだプロジェクトのハッシュ（`ProjectBackend::note_project`。`tz_guide` は版）で作り、言わなかった答えは入れません（引き当ての読み込みとツールの読み込みの間の保存で、答えが別の入力に付かない）。1 行の上限は行末の `\r\n` を除いた payload の長さです。実行中か待っている呼び出しの ID は、すぐに答えるメソッドを含むすべての要求で使えません（`Session::in_use`）。`isError` の結果の本文も `cap_text` で切ります。`tz_fix_plan` は、パスとメッセージだけから根を消し（`scrub`）、差分と `edits` の置き換えの文字列はソースのまま返します。`tz_callers` の `uses_type` は `kg uses` と同じ関係（`query::USE_RELATIONS`）を辿ります。`resources/read` は URI をツールの引数に写し（`resources::tool_request`）、`tools/call` と同じにツールの `inputSchema` で確かめます。`tz_guide` の版は根の `Tsuzuri.toml` の `edition`（`guide::project_values`、ないときは最新）で、答えのキャッシュの鍵にも入ります。`src/mcp/` には、標準出力・環境変数・プロセス・ファイルの書き込みの API を書きません（テストが静的に検査します）。

### 言語サーバー

`tsuzuri lsp` は、LSP 3.17 仕様に準拠した stdio フレームプロトコルと `serde_json` を使用し、最大メッセージサイズ 16 MiB、ヘッダー長 8 KiB、JSON 再帰深度 128 を上限として安全に動作します。
文字位置のエンコーディングはクライアントとのネゴシエーションによって決定され（デフォルトは UTF-16）、バイト位置（byte span）からの相互変換には改行テーブルと文字境界判定を用いて、CRLF 改行やサロゲートペア・異体字セレクタ等の補助平面文字を正しく取り扱います。
クライアントから受信したドキュメント URI の表記揺れはそのまま内部で保持され、診断情報や定義ジャンプの応答において正確にクライアントへ返却されます。
`Project::load_with_overlays` は、エディタ上で未保存の編集内容や新規作成ファイルをインメモリオーバーレイとして反映し、ディスク上のソースファイルを書き換えることなくプロジェクト全体を再解析します。
ワークスペースのルートは `workspaceFolders` または `rootUri` に基づいて決定され、未指定の場合は各ファイルの親ディレクトリが使われます。ネストしたルート指定が存在する場合は最長プレフィックスが選定されます。
ファイル変更通知は 200 ms のデバウンスウィンドウで集約され、クライアントからのリクエスト受信時には必要な解析を先行して完了させます。開かれているバッファの合計サイズは 32 MiB、ファイル数は最大 1024 件に制限されています。
リーダー（受信）スレッドは有界キューを通じてリクエストを受け取り、リクエストごとのキャンセル ID を共有管理します。重い解析の開始前および応答の送信前にキャンセルの有無を確認し、標準出力には厳密にフォーマットされた JSON-RPC メッセージのみを出力します。
`analyze_modules_with_semantics` は、通常の解析結果に加えて `SemanticIndex` を構築して返します。このインデックスは型検査の完了後、定数展開・単相化・クロージャの lowering より前の段階で採取され、通常のコンパイル時には生成されません。
後続の所有権検査などでエラーが検出された場合でも不完全なインデックスを公開することはなく、以前の正常なインデックスも安全に破棄されます。サーバー機能としては、ホバー（hover）、定義ジャンプ（definition）、およびシンボル検索（symbol）を中心とした堅牢な機能を宣言・提供します。
動作は `cargo test --locked --test lsp` および `cargo build --release --locked && node tests/lsp_sessions.mjs target/release/tsuzuri` によって検証されています。

VS Code 拡張は Windows x64/Arm64、Linux x64/Arm64、macOS Arm64 に限り、`vsc/src/core.ts` の `assertSupportedHost` を起動時とツールチェーン解決の共通境界で呼びます。対象外では言語サーバー・タスク・テストを登録せず、開発用コンパイラも解決しません。デバッグ判定は同じ対応表から Windows Arm64 だけを除外します。同梱物と VSIX の作成・再検証・IDE 統合テストは、`scripts/toolchain/bundle.mjs` の対応表と `assertSupportedHost` を共有します。`vsc/src/test/core.test.ts` と `tests/platforms.mjs` が対象外の拒否、5 環境の対応表・CI マトリクス・開発/公開文書の整合を検証し、`vscode.yml` は各ホストで両方を実行します。

### REPL

`tsuzuri repl` は再コンパイル型で、JIT も入力を跨ぐ常駐プロセスも持ちません（G13 D1・D13）。セッションは受け付けた宣言とトップレベルの文の**ソース**だけで、入力ごとに「宣言 → 文 → 入力の末尾」の順に `Main.tz` を生成し、`Project::single_main` で読みます。これはファイル・マニフェスト・lockfile を読まず、`stdlib::sources_for` が選んだ std だけを足すので、同じ文字列を `Main.tz` に置いた `tsuzuri run` と同じ検査・IR になります。生成したソースはディスクに書かず、パスは仮想の `<repl>/Main.tz` です（キャッシュのキーとトラップの行に入る）。
式は `let it = (E)` と `Display.display (ref it)` の版を先に検査し、`SemanticIndex` の `it: T` の項目から型を得て値を表示します。その版が通らないときだけ `let it = (E)` の版を検査し直し、通らなかった理由が表示行の `E1005` だけなら型だけを表示します（表示できる式の解析は 1 回。G13 Phase 3）。`run_captured` は `run` と同じ `build_complete` の後、子の stdin を空にし、stdout を 16 MiB まで別スレッドで集め、stderr を別スレッドで中継し、時間制限か出力の超過で子を kill して `wait` してから `E2005` を返します。`run_with_diagnostics` はこの `run_process` の `Inherit` で、`run` の挙動（stdio の継承、stderr の中継、`--json`、`E2005` の位置）は変わりません。
動作は `cargo test --locked --lib repl::` と `cargo build --release --locked && node tests/repl.mjs target/release/tsuzuri` によって検証されています。
1 入力の待ち時間（`-O0` の新しい式で約 0.67 秒）は、解析、cache の鍵（ツールの `--version` とコンパイラの SHA-256）、Clang のコンパイルとリンク、新しい実行ファイルの初回の起動（macOS の検査）でほぼ 4 分されます（`node benchmarks/run-repl.mjs`、[性能測定](benchmarks.md#repl-の-1-入力の待ち時間g13)）。JIT（LLVM の C API の ORC）、`lli`、入力ごとの共有ライブラリを読み込む常駐ホスト、値のスナップショットは、`unsafe`・新しい crate・LLVM ライブラリの配布・値の ABI を要し、この節の方針と衝突するので採りません（G13 D13）。

### スクリプト実行

`tsuzuri script FILE [arguments...]` は `run` と同じ経路で、`Project::load_script` が FILE（名前は問わず、シンボリックリンクは辿る。`.tt`・`.tc` は `E2000`）を `Project::single_main` の `Main.tz` にする。FILE の実際のパスを保つので、診断・トラップの行・キャッシュのキーはそのパスを使い、同じスクリプトの再実行はキャッシュに当たる。実行ファイルの入口の検査（`E2004`）はパスのファイル名ではなく入口のモジュールの相対パス（`Main.tz`）を見る。FILE より後ろの引数は `run_with_arguments` が子プロセスへそのまま渡す（`run` は空）。shebang 行は lexer の規則で、`tsuzuri fmt` は行をそのまま出力の先頭に写す。動作は `cargo test --locked --lib skips_a_shebang`、`--test formatter keeps_a_shebang_line`、`--test lsp a_shebang_line_keeps_positions` と `node tests/script.mjs target/release/tsuzuri` によって検証されています。

### フロントエンド

プロジェクト内の全ファイルのシグネチャを先行して収集するため、宣言の記述順序やファイル順序に依存しない設計となっています。
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
網羅性の警告 `W1003` は後述のパターン網羅性解析から生成され、シャドーイング等の字句スコープ警告 `W1004` はデフォルト無効の検査として実装されています。非推奨の警告 `W1005` は、型検査の最後（`diagnostics.check()` の後）に、`semantic::collect` の参照と宣言の説明の `@deprecated` の対応から作ります（前述の「非推奨と W1005」）。`--deny-warnings` オプションを指定した場合は、コード生成へ進む前にビルドを失敗させます。
暗黙コピーの警告 `W1006` は型検査の後段で生成されます。`copies::sites` が具体化およびクロージャ降格後のモジュールに対して所有権チェッカーを収集モードで再実行し、消費的な読み取りの中でデストラクタ呼び出しを要する Copy 値の複製箇所を、位置・型・複製種別とともに抽出します。単一使用・最後の使用・非重複の match 束縛で生成が move する位置は列挙しません。union payload 内の field へさらに射影する match 束縛は、部分 union move の生成が未対応のためコピーを維持し、一覧にも残ります。
`main.rs` は `--warn implicit-copy` が指定されている場合にのみ、配列やリストの暗黙複製を警告として報告し、LSP は同一の複製情報を `textDocument/inlayHint` のインレイヒントとしてエディタへ返却します。
デバッグビルド時の LLVM emitter は、ユーザーコードの式から発生した暗黙の `clone_value` 呼び出しをすべて記録し、そのすべてが事前の一覧に含まれていることをコード生成の最終段階でアサートします。

テスト宣言 `Program.tests` は通常の名前空間から隔離され、検査時に unit を返す内部呼び出し可能関数および `CheckedModule.tests` のメタデータへと変換されます。
テスト関数由来の属性 `FunctionOrigin.test` は局所的に生成された補助関数へも伝播され、共有されるジェネリック関数の特殊化実装は 1 つだけ生成されてルート集合からの到達性によって取捨選択されます。
通常ビルドでは公開宣言、エクスポート関数、エントリーポイントが到達性のルートとなりますが（native の実行ファイルはエクスポート関数とエントリーポイントだけ）、テストビルドでは選択されたテスト関数群がルートとなり、同一の `reachable_functions` アルゴリズムが適用されます。
テスト用内部関数のシンボル名には `@tz.fn.Module.$test.index` という決定的な命名規則が用いられ、テストランナーのみが `tsuzuri_test_count` および `tsuzuri_test_run` を公開関数としてエクスポートします。
ネイティブの C エントリーポイントは `strtoull`、errno、および endptr を用いて指定されたテストインデックスを厳密に検証し、WASM 側の Node.js エントリーポイントはインポートが空であることを確認します。
`driver::run_tests` はテストランナーを 1 回だけビルドし、上限付きの並列ワーカープロセスを用いて各テストを独立したサブプロセスとして実行します。30 秒のタイムアウトに達したテストプロセスは安全に終了・待機され、実行結果は元のテスト宣言順序へ並べ直されて出力されます。
`tsuzuri test --index N -g -o PATH` の `driver::build_debug_runner` は、テスト N だけをルートにした IR を `emit_test_runner_with` の `TestRunnerOptions::debug` で DWARF 付きにし（テストの本体の名前は `<モジュール>.test@<行>:<列>`。テストを呼ぶ `@tsuzuri_test_run` は `@main` と同じく artificial な `DISubprogram` を持ち、呼び出しに位置を付けるので、`-O1` 以上でテストが inline されても行が残ります）、C の入口とランタイムはデバッグ情報なしの別オブジェクトにして `-g` でリンクし、macOS では `dsymutil --flat` で `PATH.dwarf` を作って `publish_outputs` で置きます。実行はせず、ランナーの絶対パスと引数 `0` を出力します（G16 Phase 2）。VS Code 拡張の Debug のテストプロファイルはこれを CodeLLDB で起動し、終了コードで成否を報告します。

`tsuzuri test --coverage`（G18 Phase 2）は LLVM のカバレッジ形式や gcov を使わない自前の計装です。`coverage::plan` が単相化・ラムダ持ち上げ後の利用者関数（標準ライブラリ、`FunctionOrigin.test` を持つテスト本体とその補助関数、組み込み・ケース・export のラッパーを除く）の型付き本体を走査し、関数本体・`if` の両枝（`else` のない `if` が生成する `()` は除く）・`match` の節の本体・ループ本体・`try` のハンドラー・`&&`／`||` の右辺を (ソース, span, 種類) の昇順に番号付けした region と、各式の開始位置とその時点で最も内側の region の組（point）を作ります。
`llvm::emit_test_runner_covered` はこの計画を `Globals.coverage` に置き、`FunctionEmitter::cover` が region の block の先頭（`&&`／`||` の右辺は `short_circuit` が右辺を評価する block の先頭）で `atomicrmw add ... monotonic` を `@tsuzuri_coverage_counters` の要素へ出します（同じ span の特殊化はカウンターを共有し、`lookup_match` の定数表は使いません）。呼び出しを省く生成（恒等関数の呼び出しを引数に置き換える `call_specialization::is_identity`、既知のクロージャの解決で恒等関数を見通す `transparent`、末尾の自己呼び出しの引数で `x + y`／`x - y` だけの 2 引数関数を加減算にする `tail_arguments`）は、`FunctionEmitter::cover_call` で呼ばれる側の本体の region をその場で数えるので、呼び出し側がテストでも数は呼び出しと同じです。計画がない出力は 1 byte も変わりません。
C の入口は `-DTSUZURI_COVERAGE` のときだけ、テストが成功したらカウンターを `TSUZURI_COVERAGE_FILE` へ native の byte 順で書きます。`driver::run_tests` は成功したテストのファイルだけを飽和加算で合算し（失敗はカウンターを書く前に終わるので除外数として数える）、`coverage::files` が行ごとに point の region の最大値を取り、`render_lcov` が lcov を書きます。

ベンチ宣言 `Program.benches`（G18 Phase 1）はテストと同じく通常の名前空間から隔離され、`$bench.<index> ($iterations: i64) -> i64 = { let $case: i64 -> i64 = 本体; $case $iterations }` という生成関数と `CheckedModule.benches` になります（本体の型の不一致は `$case` の注釈により本体の位置の `E1003`）。由来 `FunctionOrigin.bench` は補助関数へ伝播します。
`polymorph::specialize` はベンチ関数の要求を、ほかのすべての特殊化（Drop の固定点を含む）が終わった後に出すので、通常ビルドとテストビルドの `$mono.N` の番号はベンチの有無で変わりません。この後半で初めて見つかった Drop 型の drop 関数は `CheckedModule.bench_drops` に記録され、`reachable_functions` はベンチ実行器のときだけそれらを根にします（ベンチ専用の型の drop glue は通常の成果物に出ません）。ただし `$lambda.N` や `$instance.N` のように関数の総数から付く生成名は、テストを足したときと同じく、ベンチや `Bench` モジュールの読み込みで番号がずれます。
`program_reach` は `Restricted { clock, counter }` で `Bench.now` と `Alloc.__counter` を直接読む関数を記録します（G18 D3・F14）。根を指定しない Console・Library では、公開の利用者関数・export・入口からの到達集合と逆向きの到達を求め、制限された資源に届く公開の補助関数を、export でも入口でもなければ根から外して再計算します。入口・export・選んだテスト・Drop からなお到達し、出力がその資源を提供しない場合は、`restricted_error` が資源の種類ごとに根からの最短経路を幅優先探索し、最後の利用者関数の次へ向かう式へ `E1018` を出します。`Bench.now` の文面と位置は変更せず、どちらにも到達しないプログラムの IR も変えません。
`llvm::emit_bench_runner`（`Entry::BenchRunner`）は選んだベンチ関数を根にして、`@tsuzuri_bench_count` と、index で分岐して `call i64 @tz.fn.<Module>.$bench.N(i64 %iterations)` を返す `@tsuzuri_bench_sample(i32, i64)`（不正な index は -1）を出します。組み込みの `Bench.now` は `call i64 @tsuzuri_bench_now()` に下がり、宣言がベンチ実行器以外の出力に残れば `emit_program` が `E1018` を返します。`Bench.consume` は値を entry block の alloca に store し、その pointer を `asm sideeffect "", "r,~{memory}"` に渡してから `drop_value` します。
`runtime/bench-runner.c` は `tsuzuri_bench_now`（macOS は `clock_gettime_nsec_np(CLOCK_UPTIME_RAW)`、Linux は `_POSIX_C_SOURCE` 付きの `CLOCK_MONOTONIC`、Windows は `QueryPerformanceCounter`）と、引数 `INDEX SAMPLES TARGET_NS` を厳密に検査して（不正なら終了コード 2）反復回数を倍々に較正し、予熱の後に `iterations N` と `sample NS` の行を出す `main` を持ちます（負の時間は終了コード 3）。
`driver::run_benches` はテストランナーと共有する `compile_native_runner`（C の入口と必要なランタイムをリンク）で実行器を 1 回だけ作り、ベンチを 1 件ずつ別プロセスで順に実行します。標準出力と標準エラーは読み取りスレッドが読み切り（パイプが満杯でも子を止めない）、300 秒で kill と wait をします。出力の形と件数を厳密に検査し、1 回あたりの時間（`ns / n / 1e6` ms）の中央値・最小・最大を報告します。合否の閾値はありません。

`--allocs` のときだけ `emit_bench_allocation_runner` が Counting の別の IR を生成し、`@tsuzuri_bench_counts(ptr)` が counter の 0・1・3 を書きます。`bench-runner.c` の `TSUZURI_BENCH_ALLOCS` 版を別にリンクし、1 反復で予熱した後、1 反復の増分 A と 2 反復の増分 B の差を取ります。unsigned の差の前に B ≥ A を検査して負の差分を終了コード 5 にし、Rust は厳密な 2 行 `alloc_calls N`・`alloc_bytes N` を読みます。失敗は bench の `allocation run: ...` にして時間の結果を捨てます。計数なしの IR と JSON/text は変わりません。

プロパティテスト（G18 Phase 3）は std の opt-in モジュール `Gen` だけで書かれています。`Gen<'a>` は `Source -> ('a * Source)` を持つ不透明なレコードで、`Source` は `(Random.Pcg * [i64u] * bool * Vec<i64u>)`（乱数、再生する選択、再生中か、記録した選択）の組です。生成器はすべての乱択を `choose`（上限 0 の選択は記録しない）で記録し、探索では seed と値の番号を stream にした `Random.pcg` の提案を、再生では記録（尽きたら 0）を上限で切った値を使います。`shrink` は記録を短縮・縮小した候補を再生し、性質が失敗し続けてかつ短長辞書順でより単純な記録だけを採用します。
seed は std 専用の組み込み `Gen.__seed()` で、`emit_builtin` が `Globals.seed`（`llvm::TestRunnerOptions.seed`、なければ `DEFAULT_PROPERTY_SEED`）の定数を返す関数を出します。`Gen` を書かないプログラムでは読み込まれないので、型検査と IR は変わりません。
テストランナーは `run_captured` で子プロセスの標準エラーを読み取りスレッドで読み切り（末尾 64 KiB を保持）、失敗したテストの `TestResult.output` にします。WASM のテストランナーは、`Gen` の関数を含むモジュールだけを `debug_output` 付きで出力し、`runtime/test-runner.mjs` は `tsuzuri_debug.write` だけをインポートとして許して `writeSync(2, ...)` で書きます。

`Debug.print` および `Debug.trace` は通常の標準ライブラリ関数として提供され、`Display` が生成した所有文字列を非公開の組み込み関数へと渡します。
ネイティブ環境では厳密な UTF-8 変換と `runtime/debug.ll` の `write(2)` ループを用い、出力完了後に変換前後の所有バッファを確実に解放します。
WASM のデフォルト動作では UTF-16 の表示結果バッファを解放するのみですが、オプトイン設定時には UTF-8 バッファを `tsuzuri_debug.write` へ同期転送して解放し、改行処理はホスト環境へ委ねます。
`llvm::EmitOptions` および `emit_with_options` が出力制御フラグを受け取り、従来の `emit`／`emit_target` はデフォルト値を指定するラッパーとして互換性を維持しています。

`llvm::emit_with_trap_info` はソースマップを明示的に受け取り、IR と `TrapSite` の対応情報を返します（デフォルトの API では計測処理を行いません）。
有効化されている場合にのみ、生成される関数呼び出しやトラップ命令に一時的なソースマーカーが付与され、`llvm_traps.rs` が生成 IR の関数、呼び出し命令、および引数リストを走査します。
ランタイムの一部（`recursive.ll`・`display.ll` など）は、直前のテキストの最後の行にそのまま続けて付け足されるため、前の関数の閉じ括弧と同じ行に `define` が来ることがあります（`}define ...`）。走査は閉じ括弧とその残りを別の行として読みます。読まないと、その関数は前の関数の一部になり、コンテキスト引数を受け取らないまま本体で使います（`use of undefined value '%tz.context'`）。間に別の部分（`Mutex` と並列タスクを使うときの `sync.ll` など）が入るプログラムでは行が分かれていて、剪定でその部分が消えると共有されるので、PB03 で直しました。
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

### 借用の表記

キーワード記法（`ref`／`ref mut`／`deref`）と Rust 互換の記号記法（`&`／`&mut`／`*`）は、構文 AST 上の `Notation` の違いとしてのみ保持されます。型検査の段階で同一の型付き IR である `Borrow` および `Dereference` へと lowering されるため、所有権チェッカーおよび LLVM コード生成器が元の表記の差異を意識することはありません。
両記法から生成される LLVM IR が完全に一致することは、`tests/borrow_syntax.rs` によってネイティブおよび WASM の双方でテストされています。
キーワード `ref` は被演算子に対して期待型を与えずに型付けを行い、静的な型が既に参照型である場合は `Dereference` を自動挿入して 1 段階の貸し直し（再借用）を行います。
型変数の段階では貸し直しは行われません。型が未確定（`Infer`）の被演算子は `undecided_borrows` に記録され、関数の `finish` 処理でデフォルト型が適用された後に参照型へと解決された場合は `E1015` エラーとなります。推論結果に基づいて事後的に型付き木の形状を改変することはありません。
字句解析器では `ref` および `deref` を予約語として扱います。実引数位置における記号記法については、直前の空白の有無と被演算子との隣接関係に基づいて、前置の借用演算子（`f &x`）であるか二項ビット演算子（`a & b`、`a&b`）であるかを正確に判別します。

関数呼び出し、パイプライン演算子 `|>`、およびアクティブパターン認識器の実引数は、`Checker::argument` において期待型に合わせて借用、再借用、および参照外しが自動的に補完されます。`xs |> f a` では、`Checker::apply_piped` が `xs` の型を `f a` の次の引数へ先に当てはめ、`a` に書いたラムダの引数の型を決めます。`ref mut xs` や `let mut` の配列が排他スライス `ref mut [T..]` の引数へ渡るときは、スライスへの変換を呼び出しが完結してから `coerce_slice_argument` が行うので、`apply_piped` は要素の型だけを合わせます（`ref mut xs |> Array.write 0 1`）。
変数名、フィールドアクセス、配列添字アクセス、および明示的参照外しは、まず元の型で型検査された後に既存の `Borrow` や `Dereference` へと変換されます。
引数位置に置かれた関数呼び出しは `call_expression` で検査され、戻り値の参照の階層や可変性を保持した期待型を伝播させたうえで補完処理が行われます。
これにより、数値リテラルやジェネリックな式の文脈依存型推論が損なわれるのを防ぎ、引数式が二重に検査・評価されるオーバーヘッドを回避しています。
明示的な `Borrow` 式の型はそのまま検査され、通常の変数束縛、関数の戻り値、あるいは型が未確定の汎用値引数に対して推測によって勝手に借用を生成することはありません。
参照型であることが静的に判明している引数に対しては、排他参照を move することなく再借用を行い、非 Copy な参照先を値渡しとして不正にコピーできないように保護します。
所有権チェッカーおよび LLVM コード生成器は、明示記法と同一の構文木としてこれらを処理するため、競合検出、生存期間、クロージャ捕捉、および評価順序の規則が一貫して共有されます。引数検査の入口はコンパクトな振り分け処理とし、自動補完の作業バッファを再帰スタックフレーム上に保持し続けないように最適化されています。

### 制御構文

`while` ループ、整数範囲ループ、コレクション反復、および `match` 式は、それぞれ専用の型付き IR ノードとして表現されます。
ループ、分岐条件、およびパターンの型は型検査時に完全に確定され、LLVM コード生成フェーズにおいて型推論やイテレータの動的メソッド探索を行うことはありません。
タプルは構造的な値型として扱われ、型置換、メモリレイアウト計算、Copy／Capture／Send の制約判定、借用追跡、および clone／drop の処理へ再帰的に参加します。
型付き式の子要素走査ロジックが一元化されており、新しい制御構文の内部においても型置換、lambda lifting、使用回数の計数、および呼び出し特殊化の処理が漏れなく適用されるよう設計されています。

`for i = a to b` と `for i = a downto b` は、`a .. b` と同じ `TypedExprKind::ForRange` になります（`src/control.rs`）。カウンターの型は新しい推論変数で、両端と本体が決め、`Integer` を要求します。接尾辞のない整数リテラルだけなら、リテラルの既定の `i32` です。
`downto` は `step` にリテラル `1` を置き、`descending: true` とします。`-1` を置かないのは、符号なしの型が `-1` を表せず、カウンターの型が型検査の終わりまで（型変数なら単相化まで）決まらないことがあるからです。`descending` の `ForRange` を読む 3 か所が、`step` を引く向きで扱います。
`llvm_control.rs` の `range_loop` は、符号付きを従来のステップ `-1` の経路（`add -1`、`icmp sge`）へ、符号なしを `icmp uge` と `sub nuw 1`（`nuw` は proven のとき）へ下ろし、64 bit 未満は narrow な i64 のカウンターで `-1` を足します（符号なしの降順は `-1` で終わるので `nuw` を付けません）。
`ranges.rs` の `loop_kind` と `ranges_intervals.rs` の `enter_counter` は `descending` を `-1` のステップと同じ降順として証明し（区間は `[finish の下限, start の上限]`）、`const_machine.rs` の `within`／`advance` は `step` を引いて、型の最小値の下へ出たところで終わります。範囲 `a .. step .. b` は `descending: false` のままで、負のステップが降順を表します。

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
ループ本体に入った値が元のビット幅に収まっているという事実のみを `llvm.assume` を通じてオプティマイザへ伝え、ユーザーの算術式に、証明のないオーバーフロー抑止フラグ（`nsw`／`nuw`）を付与することはありません。ループカウンターの増分だけは、形から証明できるので `TSUZURI_RANGE_FLAGS=proven` のとき旗が付きます（後述の「整数の範囲証明」）。
64-bit および 128-bit の単位刻みループは終点値を処理した時点で確実に停止し、任意のステップ幅を持つループはオーバーフロー検出 intrinsic と範囲比較を組み合わせて停止判定を行います。
密な整数定数の match 結果に対しては、最大 256 要素かつ密度 1/2 以上の条件を満たす場合に静的ジャンプテーブルを生成し、それ以外の整数定数選択は `switch` 命令へ、複雑な構造・ガード・アクティブパターンを含む分岐は順序付きの条件分岐へと lowering されます。ジャンプテーブルの範囲外の値も明示的な既定節（default）へ確実に分岐します。
代入が 1 箇所のみの小さな整数リダクション（集約）ループには LLVM のループ展開（unroll）ヒントを付与しますが、固定の展開数、特定 ISA、あるいは演算の再結合を強制することはせず、浮動小数点のリダクションやループ依存性を持つ整数の攪拌処理（ミキサー）には適用しません。
ループ制御用のメタデータは、同梱の数値ランタイムが使用するメタデータ ID 範囲と完全に分離されており、すべての LLVM IR を決定的に生成します。

### コンピュテーション式

`.tc` ファイル内に定義されたすべての関数名を順序付き集合として収集し、そのファイル名をビルダー名として扱います。
`@alias name`（D-43）は `Program::aliases` に保持され、`computation::collect_all` が `.tc` の宣言だけを別名 → ビルダーの鍵の表（`Names::builder_aliases`）に集めます（`.tz`・`.tt` の `@alias` は `E1018`）。`Names::builder_name` が `{` の前の名前を解決します。まずビルダーのモジュール名（名前空間付きを含む）として探し、当たらなければ別名として探します。別名は層ごと（利用者 → std）に、ビルダー自身の名前がその参照元から見える候補だけを数え（`builder_in_sight`。2 つの `using` が同名のビルダーを持つときは、どちらも見えるものとして数えます）、1 つなら採用し、複数なら `E1004` です。本体が空の `name {}` は別名があれば別名を選び、なければ、その名前のレコードがない小文字の名前を宣言のないビルダーとして `E1018` にします（`empty_block_is_computation`）。別名は `computation::expand` の `resolved_builder` がビルダーのモジュールの鍵へ書き換えるので、以降の `lower` と型付き IR は名前を書いたときと同じです。構文木・型付き IR・ランタイムに別名のための構造はなく、opt-in std モジュールを隠す `Names::choose` の規則も適用しません（opt-in の読み込みは `OptIn::aliases` が決めます）。
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
空の `Name {}` 構文は、収集済みビルダーに該当名（別名を含む）が存在する場合はビルダー式を優先し、存在しない場合は既存の空レコードとして解釈されます。
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

### 多相性

明示的なシグネチャに現れる型変数はリジッド（rigid：硬直型変数）として固定され、各関数の参照箇所で導入される推論変数のみが単一化（unification）の対象となります。
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
LLVM では、配列の添字アクセス、リストの走査（左右の現在の chunk に残る slot の数と残りの組数の最小を run として、run を数える内側の loop を run ごとに回す二重 loop）、およびタプルのフィールド GEP 命令を用いてコード生成を行い、最初の不一致を検出した時点で短絡評価して高速に終了します。
各要素の比較には既存の借用 ABI が用いられ、部分適用された比較メソッドに対しても残りの引数が順次適用されます。型辞書、汎用 boxing、要素の不要なコピーは一切発生しません。

### 導出（deriving）
`derive::instances` は、record や union の宣言から通常の `InstanceDecl`、式、およびパターン AST を自動合成し、手書きのインスタンスと完全に同一のコヒーレンス検査、型検査、所有権検査、および特殊化パイプラインを通過させます。
合成された識別子や内部クラス参照は provenance（由来情報）によって区別され、同名のユーザー定義モジュールへと誤解決されることはありません。診断メッセージは元の `deriving` 記述の span を正確に指し示します。
自動導出された構造的要件に限定して同一クラスの再帰呼び出しを許容し、`Default` インスタンスの循環定義は厳格に拒否します（一般的な循環インスタンスは引き続き `E1017` エラーとなります）。
`Hash` のプリミティブ演算では、ビット幅に応じた整数の load、shift、xor、および wrapping multiply を行い、decimal 型では `numeric.c` の decode/encode 処理を共有します。
合成されたコレクションヘルパー内では `StructuralHash` および `StructuralDisplay` を使用し、子要素の具体的なメソッドを通常の到達可能性解析に含めます。
値の文字列表現では内部の `DisplayQuoted` 組み込み関数を具象型へと解決し、`runtime/display.ll` が UTF-16 の引用符処理と文字列片の一括結合を担当します。
`Encode`／`Decode` の導出（D08）は、レコードでは `$object{i}`（encode）と `$field{i}`／`$failed{i}`／`$error{i}`（decode）の束縛の連鎖、union では 1 段の `match` を合成します。std の補助関数（`Json.encode_field`、`Json.decode_field`、`Json.case_index` など）と `Result.is_error` などは `QualifiedFunction` のモジュールキー（`Json.begin_object`）で、`Result.Ok`・`Maybe.None` などの case はキーパス（`::Result.Result`）で参照するので、利用者の名前空間や同名の宣言に解決されません。深さはフィールド数・case 数に比例しないため、128 フィールドや 128 case も深さ 128／4096 節点の上限に収まります。フィールドと case の `@json "名前"`（`Parameter::json`・`UnionCaseDecl::json` の `JsonName`）はキーとタグの文字列だけを変え、`derive::json_names` が `Encode`／`Decode` の導出の無い型での使用と名前の重複を `E1025` にします。formatter は属性のトークンをそのまま並べ（span は fingerprint から除く）、docgen は属性ごと表示します。成分の instance の欠落は std の補助関数を経由して単相化の制約伝播で見つかるため、`specialize` の正規化も `derived_error` で `E1025` に読み替えます。
インスタンスの重複判定（`Classes::instances`）は、ヘッドをクラスと最外の型構築子（スカラーの種類と幅、record／union の id、配列・リスト・`Vec`・`Task`、タプルの長さ。型変数や高階のヘッドは `None`）で索引し、同じ構築子か `None` のヘッドとだけ単一化します。1024 組の上限は単一化した組だけを数えるので、std の `Json` が多数のインスタンスを持っても、別々の record に導出したインスタンスが何百あっても予算を消費しません。
Hash の canonical stream 仕様および文字型ごとの引用規則の詳細は言語仕様を参照してください。なお、`numeric.ll` は生成スクリプトから自動生成されるため、手動で直接編集してはなりません。

### 文字列補間と書式指定

`$"a{x}b"` および `u8$"..."` 形式のリテラルは、字句解析器によって `InterpolationStart`、`InterpolationMiddle`、`InterpolationEnd` の各トークンに分割され、構文解析器によって `ExprKind::Interpolated`（固定文字列片と `InterpolationHole { value, spec }` の列）へと変換されます。字句解析器は開かれた埋め込み穴を `holes` スタック（`OpenHole { utf8, depth, start }`）で管理し、括弧のネスト深度が 0 の位置にある `}` または `:` によって穴を閉じます。埋め込み穴は同一行内に収める必要があり、穴の内部にコメントを記述することはできません。埋め込み穴を含まない文字列リテラルは通常の文字列トークンとして維持されます。リソース制限として、1 つのリテラルあたり最大 1024 個の穴、幅と精度は最大 4096、ネスト深度はパーサーの標準上限が適用されます。
型検査（`interpolation`／`interpolation_hole`）は `TypedExprKind::Interpolated(TypedInterpolation { texts, holes })` を構築し、型はリテラルのプレフィックスに応じて `string` または `utf8string` となります。`TypedHole.operand` は常に共有借用 `ref U` となります。穴が場所かどうかは書き方ではなく型付けした式で決まります（`hole_operand`・`hole_place`。`TypedExpr::is_place` と、平たい定数の一部 `C[i]`・`C.field`）。場所は `ref` 引数と同様に借用され、場所でない参照はそのまま再利用（排他の参照は共有に再借用）され、それ以外の評価結果は `BorrowOperand` による stack の slot に置かれ、文字列結合完了時点で安全に drop されます（`.length`・文字列の添字・呼び出し結果のフィールドや添字、引数のない関数呼び出しや単独の union case のように値を生成する式も一時値として扱われます）。平たい定数の一部は `constant_part` が見分け、fold 後の静的データの借用を保ちます。穴の式は左から右へと順次 1 回ずつ評価され、表示自体は値の所有権を消費しません（一時値の評価中のムーブには通常の所有権規則が適用されます）。適用されるメソッドは原則として `Display.display`（カスタム書式の場合は `Format.format`）であり、リテラル自身と同一の文字列型の穴や、符号・精度・型指定を持つ数値の穴に対してはメソッド呼び出しを行いません。
LLVM コード生成（`llvm_display.rs`）では、すべての穴を左から右へ評価して `Piece`（データポインタ、長さ、所有者、ASCII 判定、パディング情報）を構築し、リテラルの固定長、各穴の展開長、およびフィル文字の必要数を合算して、最終結果を `@tz.string.allocate` または `@tz.utf8string.allocate` によって 1 回のメモリアロケーションで確保します。固定文字列片はメモリコピーで書き込まれ、穴の内容は各値の表示バッファから直接コピーされます（リテラルと同一型の文字列の穴は Display を経由せず元のバッファを直接読み込み、数値の ASCII 出力は UTF-16 結果の場合に `@tz.format.widen` で拡張しながらコピーします）。結合完了後、すべての一時値が解放されます。`TSUZURI_STRINGS=compact` の `string` の補間は `compact_interpolation` が書き、結果の形式（Latin-1 か UTF-16）を実行時に穴の形式から決めて `@tz.format.allocate`・`put`・`fill` で書きます（確保は 1 回で、数値の ASCII 出力は広げずに byte のままコピーします）。`u8$` リテラルにおいては、Display が生成した UTF-16 の結果を `@tz.utf8string.from_string` で UTF-8 へ変換し（孤立サロゲートを検出した場合はトラップ）、変換元のバッファを解放します。
数値の書式指定処理は、ホスト環境の `printf` を使用せず、`src/runtime/numeric.c` の `tz_soft_format_spec` が多倍長の係数から直接最近接偶数丸めを行って ASCII 文字列を出力します（`numeric.ll` は `python3 src/runtime/generate.py`、`math.ll` は `python3 src/runtime/generate_math.py` で再生成します）。整数（`i8`〜`i128`、`i8u`〜`i128u`）の書式指定は `tz_soft_format_spec` ではなく、`runtime/integer.ll` の `tz.integer.spec`／`tz.integer.spec128` が同じ `flags` を受けて書きます（`f`・`e` と精度は整数には指定できません）。バッファは 160 byte で、テキストは末尾から前へ書くので、`number_piece` は末尾のアドレスを渡し、返った長さから先頭を求めます。`flags` は bit 0 で `+` 符号を、bit 4 以降で出力スタイル（0: デフォルト、1: `f`、2: `e`、3: `x`、4: `X`、5: `o`、6: `b`）を表現します。作業バッファには、`format_capacity` が型と精度から算出するサイズ（512 バイト以下）のエントリー `alloca`、または `@tz.alloc` で確保されたヒープ領域が使用されます。パディング用の補助関数群は `runtime/format.ll` に配置され（スカラー数計数の `@tz.format.scalars.utf16`／`utf8`、フィル書き込みの `@tz.format.fill.u16`／`u8`、ASCII 拡張の `@tz.format.widen`）、前後のフィル数は LLVM が穴ごとに正確に計算します。幅のカウントは Unicode スカラー数に基づいて行われます（サロゲートペアは 1、孤立サロゲートも 1、数値の ASCII は文字数そのもの）。余白の不足分は、デフォルトで数値は左揃え（右パディング）・その他は右揃え（左パディング）となり、中央揃え `^` は不足分の半分（端数切り捨て）を前方に配置します。

### Format クラス（D07 Phase 2）

`Format<'a>` は組み込み型クラスの 27 番目として定義されており、メソッド `format :: ref 'a -> ref string -> string` はコンパイラ組み込みの実装（`Operation`）を持たず、ユーザー定義の `instance` 実装に委ねられます。インスタンスのヘッド型は、`validate_format_instance` によって現在のプログラムで宣言された record または union（型引数は自由）に限定され、それ以外の型に対する実装は `E1016` エラーとなります（到達不可能な無効インスタンスの作成を防ぐためであり、`validate_drop_instance` と同一フェーズで検査されます）。クラス名 `Format`（ユーザーによる `record Format` の定義は `E1001`）およびモジュール名 `Format`（`E1011`）は予約名です。`TypedHole.custom` は、書式指定が存在し、穴の型が record または union であり、かつ指定に符号・精度・型が含まれるか `has_format_instance` が真である場合にセットされます。このとき `method` は `Format.format` に設定され、適合するインスタンスが存在しなければ `E1005`（`no instance for Format<T>`）エラーとなります。条件付きインスタンス（`instance Format<'a> => Format<Box<'a>>`）もサポートされます。カスタム指定でない穴の書式文字列は、`check_format_specs` が推論後の型に対して妥当性を検証し（`+` は数値型、精度は float および decimal、`x X o b` は整数型）、不整合がある場合や型が未確定のまま残った場合は `E1003` となります。`Display` のみを持つ record や union に対して幅とアライメントのみを指定した場合は、従来どおり Display の出力結果に対してパディングが行われます。
`format_piece` は、借用した対象値と、検証済みの書式指定を正規の文字列表現（デフォルトのフィルや省略要素を除いた形式、例: `*>+8.2f`）にした文字列定数を、スタック上の `%tz.string` 記述子経由でインスタンスへ渡し、返却された文字列をそのまま穴の展開結果として使用します。コンパイラ側で追加のパディングや後処理を行うことはなく、幅や配置の適用はインスタンス実装側の責任となります。`std/Format.tz` に用意されている `Format.parse :: ref string -> Maybe<Format.Spec>`（字句解析器と同一の構文規則に従い、不正な文字列、先頭が `0` の幅や精度、4096 超過、禁止文字などを `None` として拒否）および `Format.pad :: ref Format.Spec -> string -> string`（スカラー数に基づくパディング）は、このための標準ライブラリ実装であり、コンパイラコアから直接参照されることはありません。なお、実行時に動的構築された書式文字列、`deriving (Format)`、`#` や `0` フラグ、ロケール依存処理、および書記素クラスタ（grapheme cluster）幅の考慮は現時点で未実装です。

### 文字列補間の編集支援と検証

VS Code 向け TextMate 文法では、`string.interpolated.tsuzuri`、`meta.embedded.interpolation.tsuzuri`、および `constant.other.format-spec.tsuzuri` の各スコープが付与されます（`vsc/syntaxes/tsuzuri.tmLanguage.json`）。LSP は穴の内部の式を型および定義のインデックスに含め、固定文字列片や書式指定の位置では不要なコード補完を抑止します。フォーマッタは穴の式の前後に余分な空白を挿入しません。
検証は `cargo test --locked --test string_interpolation` および `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri string_interpolation` で実施されます。後者は書式仕様、パディング、および `Format` インスタンスの動作を、独立した JavaScript 参照実装およびネイティブ／WASM の `-O0`／`-O3` 出力と照合します。`tests/lsp.rs` および `vsc/src/test/grammar.test.ts` がエディタ連携機能を担保しています。

### 関数シグネチャの制約行

インデントされた `@'T : Class, #function` 形式の記述は、`ConstraintExpr` において型クラス名と関数要件名に明確に区別されて保持されます。型クラス要件は従来の `Constraint` へと lowering され、インラインの `Class<'T>` 記法や前置制約とも共存します。
関数要件は `MemberConstraint` として対象型、関数名、参照元モジュール、および呼び出し時の関数型を正確に保持します。`'T.function` の構文ノードは識別子を内部でボックス化し、専用パーサーへ分離することで、構文解析時のスタック消費量を抑制しています。
関数本体の収集後、`solve_members` が関数要件をコールグラフ上で不動点に達するまで伝播させ、対象型が具体的なレコードや union に確定した要件を `Names::member_function` によって定義元モジュールから解決します。
要求される関数型と選択されたシグネチャとを単一化し、呼び出し元の戻り値型を確定させた後に `finish` および網羅性検査を行います。
数値型のデフォルト型決定は、確定可能な関数要件をすべて解決した後に適用され、その後に残った要件に対して再度解決を試みます。
関数要件は重複を除いて関数あたり最大 128 件までに制限され、伝播される型情報にも既存のネスト深度および要素数の上限が適用されます。
抽象メンバー参照に対する再帰呼び出し検査では、同一名の可視なモジュール関数を保守的な候補として扱います。
単相化フェーズにおいて可視性、存在性、および関数型が再確認され、通常の関数参照へ変換された後にクロージャ捕捉、所有権検査、および closure lowering が適用されます。
`private` の可視性検査は常に参照元モジュールのコンテキストを維持し、呼び出し元や具体型のモジュールへと権限が不当に移転することはありません。
型変数のまま未解決の関数参照が LLVM コード生成へ渡されることはなく、専用の動的ランタイムや実行時探索機構を追加することもありません。

### 整数 intrinsic

`Int.*` 関連の関数は通常の多相組み込み関数として定義され、各整数ビット幅に応じた LLVM の直接命令および intrinsic へと lowering されます。
ビット数カウント処理におけるゼロ入力時の未定義動作フラグは false に設定され、ビット回転数はビット幅に合わせて明示的にマスク処理されます。チェック付き演算（`checked_*`）は既存の `Maybe` union の構築処理を共有します。
`i128` の checked および saturating 乗算は、64-bit limb の部分積とキャリーフラグを解析して上位 128-bit のオーバーフローを厳密に検出し、符号付き演算では絶対値の許容限界も正確に判定します。これにより、未同梱の外部ランタイムライブラリ（`__muloti4` など）に依存することなく、自己完結したコード生成を実現しています。
整数のべき乗はバイナリ累乗法（二乗法）で実装され、最後の指数ビットの処理後に不要な二乗算を行わないように最適化されています。
`i32` の `Int.min`・`Int.max`・`Int.leading_zeros` が使う `llvm.smin.i32`・`llvm.smax.i32`・`llvm.ctlz.i32` は、f64 などの表示や文字列化のために連結する `numeric.ll` も宣言します（整数の表示は `integer.ll` が書き、宣言を持ちません）。同じプログラムでは生成コードの宣言だけが残ります（後述の `unique_declarations`）。`tests/integer_intrinsics.mjs` は、全幅の `Int.*` の参照値との照合に加え、これらを `IO.write_line`・文字列補間・`to_string` と組み合わせたプログラムを、ネイティブと WASM の `-O0`／`-O3` で参照値と照合します。

### 消費型コレクション更新

`Array.__set`（std の `Array.update_at` の本体。`__` 付きの名前は利用者向けではなく、F# 互換の `Array.set` は排他スライスをその場で書き換える別の関数です）、`Array.update`、`Array.swap`、ならびに `List.cons`、`tail` は、通常の多相 `BuiltinInstance` として提供されます。`Array.__set`・`Array.update`・`Array.swap` の引数は（添字、値または関数または 2 つ目の添字、配列）の順で、配列が最後です。
既存の組み込みラッパーおよび emitter の境界検査、drop、関数適用の仕組みを共有し、個別の専用フック関数は持ちません。
呼び出し側の Copy／move およびスタックフレームからのヒープ移送が完了した所有バッファに対してインプレース更新を行うため、組み込み関数内部でコレクション全体を不必要に再複製することはありません。
`update` における更新前の旧要素および関数のクロージャ環境は、所有権を持つクロージャ呼び出しへと安全に引き渡されるため、適用完了後に二重解放されることはありません。
`tail` は空リストを検査（`BoundsCheck`）したうえで先頭要素を drop し、先頭の chunk はその最後の要素と一緒に解放します（chunk ごとの縮小や再配置はしません）。確保・解放の回数は、E2E テストにおいて単一使用、旧バッファの再利用、およびスタック移送の各ケースで厳密に検証されています。

### 遅延反復（Seq）

`Seq` は標準ライブラリの不透明な record 型として定義され、常に非 Copy（所有型）となります。`head` は `Maybe` の要素値、`step` は `Maybe` のクロージャを保持し、空シーケンスや 1 要素シーケンス（once）の生成ではクロージャ環境の動的確保を必要としません。
`Seq.next` のみが型付きの組み込み関数として内部表現を直接検証し、`head` を取り出すか、`step` クロージャを所有モードで 1 回だけ呼び出します。反復のたびにクロージャ環境全体をディープコピーすることはありません。
`defer`、`unfold`、`map`、`filter`、`to_array` は通常の std ソースとして記述されており、クロージャ捕捉および借用の伝播ルールを共有します。`filter` は述語関数に要素を共有借用として渡すことで、フィルタリング中の要素の喪失を防止します。
`for ... in seq` 構文は、型検査時に既存の Block、While、Match、Assign、Break を組み合わせた制御フローへと展開され、反復ごとに次の状態を復元してからユーザーのパターンとループ本体を型検査します。
展開時には完全修飾された組み込み関数参照が用いられるため、ローカルスコープに `Seq` という名前の変数が存在しても展開先が狂うことはありません。既存の不動点所有権解析およびループクリーンアップ機構を共有し、通常のコレクションに対する直接 `for` ループの挙動には干渉しません。
`Array`、`Vec`、`Map`、`Set` からのシーケンス生成は共有参照とインデックスを用いてステップ関数を構築し、`List` からの生成では `List.foldr` によって参照の `Vec` を 1 回構築したうえで所有状態としてシーケンスへ移送します。
検証は `cargo test --locked --test iteration_protocol` および `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri iteration_protocol` で行われています。

### 順序付きコンテナ

`Map`、`Map.Entry`、および `Set` は、標準ライブラリの不透明な record 型として登録され、構築、フィールドアクセス、パターンマッチ、および更新操作が定義モジュール内に厳密に限定されます。
型名は通常のジェネリックレコードであり、内部的には `Map` が `head: Vec<Entry<K, V>>`・`rest: Vec<Vec<Entry<K, V>>>`・`length: i64` を、`Set` が `head: Vec<K>`・`rest: Vec<Vec<K>>`・`length: i64` を保持します（`head` が先頭のチャンク、`rest` が残りのチャンクで、各チャンクは 1〜512 エントリ、隣り合う 2 つの合計は 256 を超えます）。`Vec` が持つ非 Copy 性、メモリ確保、および clone／drop の仕組みをそのまま共有し、ランタイムに新しい特殊な内部表現を追加することはありません。
チャンクの最後のキーに対する二分探索と、チャンク内の二分探索（`locate`）によって借用比較を行います。更新は、対象のチャンクを `Vec.swap` と `Vec.pop`／`Vec.push` で取り出して（`take_chunk`）そのチャンクの中でエントリを移動し、戻します（`put_chunk`）。満ちた（512 エントリの）チャンクは半分に分け、隣り合う 2 つの合計が 256 以下になったら併合します。重複キーの挿入時には、既存のキーインスタンスを維持したまま値（value）のみを置換します。
`Set.union` は両辺のチャンクの末尾から要素を比較して pop し、384 エントリずつの逆順のチャンクを作った後に反転します。`intersect` および `difference` は、（チャンク番号, オフセット）の 2 本のカーソルを進めながら借用で比較し、選択した左キーだけを `key_at` でコピーして同じ形のチャンクを作ります。コピーをローカルなチャンク参照からではなく自身の引数を持つ helper から行い、キーが含む参照の借用元情報を保ちます。600,000 要素の配列キーの積・差は既存の WASM 16 MiB 上限で検証し、比較のための不要な clone への退行を時間の閾値なしで検出します。
不透明レコードの内部には共有参照を格納できますが、排他参照（`ref mut`）の格納は拒否され、通常のレコードを経由して隠蔽格納しようとする試みも型検査によって確実に検出されます。
NLL（非字句的生存期間）における借用情報の解放処理では、生存している借用の参照先と親要素を正しく追跡し、読み出しに必要となる元の所有者情報を維持します。
検証は `cargo test --locked --test map_set --test map_set_chunks --test std_argument_order`、および `cargo build --locked` の後の `node tests/features.mjs target/debug/tsuzuri map_set` と同じコマンドの `map_set_chunks` で行います。LLDB と natvis は `head` に続いて `rest` のチャンクを列挙し、要約 `size=N` とキー順の `[0]`、`[1]`…の表示を維持します。LLDB は必要になったチャンクのヘッダだけを読み、キャッシュした境界の二分探索で子の所属チャンクを求めます。`tests/lldb_collections.py` は 128 チャンクの境界アクセス回数を検査し、全チャンクの再走査への退行を時間の閾値なしで検出します。

### ハッシュコンテナ

`HashMap`、`HashMap.Entry`、および `HashSet` も `Map` と同様に不透明な標準 record として定義されており、専用の `Type` バリアント、特別な組み込み命令、あるいは C 言語ランタイム関数を必要としません（`stdlib.rs` における予約型名と不透明レコード一覧への登録のみで完結しています）。実装は `std/HashMap.tz` および `std/HashSet.tz` の純粋な Tsuzuri ソースコードであり、`HashSet<'key>` は内部に `HashMap<'key, unit>` を 1 つ保持する構成をとります。
`HashMap<'key, 'value>` は、挿入順に密に格納されたエントリー配列 `entries: Vec<Entry>`（ハッシュ値・キー・値）、エントリーのインデックスまたは `-1` を保持するオープンアドレス法の索引テーブル `slots: Vec<i64>`、ならびに非公開フィールド `key0`、`key1`、`keyed` から構成されます。
索引テーブルは線形探査（linear probing）を採用し、サイズは常に 2 の冪乗（最小 8）、最大負荷率は 1/2 に制御され、`2 * (count + 1)` がテーブルサイズを超過した時点で容量を倍加して再ハッシュ（rehash）を行います。
キーの削除操作（`remove`）では、墓石（tombstone）マークを残す方式をとらず、理想的な格納位置との距離を計算して後続のクラスタを前方へ手繰り寄せる後方シフト削除（backward-shift deletion）を用いて索引テーブルを整理し、実データのエントリー配列からは末尾要素を空き位置へ移動する swap-remove を実行します。このため、コレクションの要素列挙順序は挿入と削除の履歴順序のみによって決定論的に定まり、ハッシュ値、テーブルサイズ、アーキテクチャ、あるいは乱数シードの差異に依存しません。
デフォルトのハッシュ関数は、`Hash.hash`（64-bit FNV-1a）の出力を `fmix64` で攪拌（アバランシェ効果）させた値を用います。これは FNV-1a の下位ビットが入力の下位ビットのみに依存する傾向があり、2 の冪乗サイズのテーブル添字としてそのまま用いると衝突が集中しやすいためです。
`with_seed` や `with_capacity_and_seed` で生成されたマップは `keyed` フラグを有効化し、シード値から SplitMix64 によって生成された 128-bit のキー（`key0`、`key1`）を入力とする SipHash-1-3（`HashMap.sip13`。64-bit ワードを入力とする純粋関数）を用いて最終的なダイジェスト値を算出します。
`randomized` および `try_randomized` は、`Random.next_u64` からシード値を取得する `IO` アクションであるため OS API のカテゴリに属し、デフォルトの wasm32 では `E2000` で拒否され、ネイティブおよび `--wasm-host wasi` 環境でのみ使用可能です（安全なシード値を取得できない場合に固定シードへ勝手に縮退することはありません）。なお、シード付きマップは固定テーブルに対する意図的なハッシュ衝突攻撃を防ぐ効果を持ちますが、元の `Hash.hash` の段階で 64-bit ダイジェスト値そのものが衝突するキーに対しては無力であり、またシード値が推測された場合は防御効果が失われます。したがって、デフォルトのマップは暗号学的な HashDoS への完全な耐性を保証するものではありません。
キーの所有権を消費しない読み取り操作（`contains_key_ref`、`get_ref`、`at_ref`、`remove_ref` など）は `ref 'key` を受け取り、`at_ref {r s}` における region 契約によって、戻り値の参照はマップ自身の借用期間のみに拘束されます。キー型の `Eq` が反射律を満たさない場合（NaN など）は、`hash_of` 内部のアサートによって安全にトラップします。`longest_probe` は理想スロットから最も離れたエントリーの探査距離を返す診断用メソッドです。なお、SIMD による群探査（F08）、テーブル縮小、および集合演算（union／intersection）は現時点で未実装です。
検証は `cargo test --locked --test hash_map` および `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri hash_map` で行われています。シード付きコンテナが OS ランタイムを要求せず、`randomized` のみが OS ランタイムを要求することは `tests/os_api.rs`、ネイティブ・WASI・デフォルト wasm32 での挙動は `tests/os.mjs` の `randommaps` で検査されています。

### Arena（C10）

`Arena.Arena`・`Arena.Handle`・`Arena.Slot` も `stdlib.rs` の不透明な標準 record として登録され、実装は `std/Arena.tz` の Tsuzuri ソースだけです。専用の `Type` バリアント、ランタイムのファイル、リンク条件は増えません。
`Arena<'a>` は値を詰めた `values: Vec<'a>`、各位置の slot 添字 `owners: Vec<i64>`、slot の表 `slots: Vec<Slot>`（位置と世代）、空き slot の列の先頭 `free` からなる DenseSlotMap 方式で、`Arena.Handle<'a>` は arena ID・添字・世代の 3 つの `i64` です。
使用中の slot の世代はハンドルと同じ 0 以上の値、空き slot は次の世代 `g` を `-1 - g`（-2 以下）、退役した slot は -1 で持ちます。ハンドルの世代は負にならないので、`position` の世代の比較だけで使用中の slot に限られます。関数値の複製で arena ID ごと複製された 2 つの arena の間でも、一方のハンドルが他方の空き slot（`position` は空き列の次）に一致して誤った値を返したり、swap-remove で空き列を壊したりしません。
`Handle` の `'a` はどのフィールドにも現れない phantom な型引数です。record 宣言の検査は、不透明な標準 record に限って公開フィールド型の検査と未使用型引数の `E1024` を免除します（判定は宣言ごとに一度だけ行う）。`src/recursive.rs` はフィールドを型引数で置換して辿るので、`record Node { edges: Vec<Arena.Handle<Node>> }` は再帰的な値レイアウトになりません。
arena ID は std 専用の組み込み関数 `Arena.__next_id`（`Builtin::ArenaNextId`、`Checker::builtin` が std の `Arena` 以外からの使用を `E1022` で拒否）が採番します。`emit_builtin` は単相の定義と大域カウンター `@tz.arena.next_id = internal global i64 0` を一つの文字列で出し、`atomicrmw add ... monotonic` で 1 増やして、結果が正でなければ `@llvm.trap` します。一意性だけが必要で、カウンターを通じて他のメモリを公開しないので `monotonic` で足ります。
既定の wasm32（atomics 機能なし）では LLVM の WebAssembly backend が atomic 命令を通常の load／add／store へ下げ、`--wasm-feature threads` では `i64.atomic.rmw.add` になります。どちらも WASM の import は増えません。Arena を使わないプログラムの IR は変わりません。
std の Arena 関数は他の std の generic 関数と同じく利用者コードから到達した要素型ごとに特殊化され、特殊化の上限（65,536 件）に数えます。検証は `cargo test --locked --test arena` と `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri arena`（`TSUZURI_TSAN=1` での並列採番を含む）で行っています。

### Rc／Arc（C10 Phase 2）

共有ポインタは `Type::Shared(Box<Type>, SharedKind)`（`Rc`・`RcWeak`・`Arc`・`ArcWeak`）で、std のソースを持たない組み込み型です。`resolve_type` が `Rc<T>`・`Rc.Weak<T>`・`Arc<T>`・`Arc.Weak<T>`（`std::` 付きも）を `Vec` と同じく宣言の解決より先に読み、`builtin_type_head` が公開型の検査・型エイリアスの展開・制約の収集で名前の解決を飛ばします。型名 `Rc`・`Arc` は `Vec` と同じく利用者の宣言に使えません（`E1001`）。関数は `Builtin::RcNew`〜`Builtin::ArcPtrEq` の 18 個で、`Builtin::shared_kind` と `SharedOperation` が `Rc`／`Arc` の同じ処理を引きます。`new` は予約語のまま、parser の `dot_member` がドットの後ろでだけメンバー名として読みます。
性質: `is_copy` は偽、`needs_drop` は真、`contains_reference`・`carries_loans`・所有権の `owned` は中の値に従います。`can_send` は `Arc` で値が `Send` かつ `Sync` のときだけ真で（Rust の `Arc<T>: Send` と同じ規則）、`can_capture` は `Arc` で値が `Sync` のときだけ真です。`Type::can_sync`（F10）は格納グラフ（`stored_all_closed`）で判定し、`Rc`／`Rc.Weak`、`Type::Handle`、`Task`、排他参照、Copy でない dyn（と借用を持つ dyn）、`Owned.Function`、`Seq`・`Gpu.Device`・`Gpu.Buffer`・`Async.Async`・`Async.Next`（`Type::is_unsync_record`）がないことを求めます。`Arc` を持つ複数のタスクが `Arc.get` の借用を通してホストのハンドルや、非 atomic な状態を同時に使うことを防ぐためです。`Atomic` と `Mutex` は `Sync` で（`Mutex<T>` は中を見ない閉じた型で、`Mutex.create` が `Send<T>` を求めます）、型変数、推論変数、型変数を頭にした適用（`Type::Application`、`'f<i64>`）と型構築子の部分適用（`Type::Partial`）は、まだ分かっていない型なので `Sync` でも `Send` でもないと扱う（`Type::is_unresolved`。`sync_here` と `can_send` が偽を返し、どちらも `Type` のすべての種類を列挙するので、新しい種類は判定を書かないとコンパイルできません）ので、`Sync<'a>` と `Send<'a>` の制約はジェネリック関数に残り（`specialize` が `normalize` の残差として持ち越します）、具体化ごとに検査されます。`Channel.bounded` や `Mutex.create` を包む関数、`Parallel.*` の結果型、利用者が書いた `Send<'a>` も同じです。関数値の型は捕捉を表さず、どの関数値も `Send` かつ `Sync` なので、`Rc` を関数値に入れないことでタスク間の非 atomic な計数を防ぎます（`Task.scope` が共有する関数値の環境は所有権の解析が所有値と証明したものに限ります）。拒否のメッセージは `holds_rc` と `holds_unsync_arc` と `holds_host_state` で選びます。`Validation::check` は排他参照を含む中身を `E1005` で拒否し、`Layouts::size` は共有ポインタを 8 バイトとして中を辿りません。
再帰の解析（`src/recursive.rs`）は共有ポインタを格納のグラフに含めます（`visit`・SCC の辺・`stored_all`・`reaches`）。型引数を変える再帰（`E1017`）は、ジェネリックな宣言を自身の型パラメーターで解析するとき（`Graph::generic_root`）だけ、同じ宣言の別の具体化への到達として検査します。具体型を根とする解析では比較しないので、`Rc.upgrade` が返す `Maybe<Rc<Node>>` が `Node` の `Maybe<Rc.Weak<Node>>` に出会っても拒否しません。ほかの根の解析が先に到達した、自身のパラメーターのままの宣言の結果はキャッシュしないので、検査は宣言の順序によりません。増え続ける展開はノード 4096・深さ 128 の上限で止まり、経路に同じ宣言の別の具体化があれば `E1017`（引数の変化）として報告します。共有ポインタは union のノードと同じくヒープへの間接なので、`Graph::shared` に記録した共有ポインタを通る循環には union を求めません。値が有限かの判定では `Shared(T)` は `T` と同じです（`record Loop { next: Rc<Loop> }` は `E1010`）。循環の中の型は再帰型になり、性質の計算は既存の `stored_all` の経路を通ります。
生成（`src/llvm_shared.rs`）: 値は `ptr` で、`canonical_type` は `rc[T]`・`rc.weak[T]`・`arc[T]`・`arc.weak[T]`、debug 情報は基底型のないポインタです。`named_types` は共有ポインタの中の値も辿るので、`Vec<Rc<Maybe<string>>>` のように共有ブロックの中にしか現れない record／union の具体化にも型定義を出します。ブロックは `{ i64 strong, i64 weak, T }` で、`T` が再帰型を格納する（`Type::reaches_recursive`）ときだけ `%tz.rec.header` の 2 語を前に置いた `{ ptr, ptr, i64, i64, T }` です。
`drop_shared` はヌル（ムーブ済み）を飛ばし、強い数を減らして 0 なら値を drop してから `release_shared_block` で弱い数を減らし、0 ならブロックを `@tz.free` します。前置きのあるブロックは、値の drop の代わりに action `@"tz.shared.drop.{rc,arc}.<T>"` をブロックに書き、`drop_pending` があれば `@tz.rec.enqueue`、なければ `@tz.rec.drop` で再帰型と同じ待ちリストに積みます。action は `drop_pending` を付けて値を drop するので、`Rc` を通る長い鎖も再帰しません。action の型は `Globals::shared_types` に集め、`llvm_recursive::emit_helpers` が再帰型の helper と交互に、どちらも増えなくなるまで定義します。組み込み関数の本体は別の `Globals` で出力するので、`emit_typed_builtin` は本体が登録した再帰型と共有ブロックの型を共有の `Globals` へ移します。
`clone_value`（関数値の環境の複製）は強い数か弱い数を 1 増やして同じポインタを返します。増加は `i64` の最大値を超えるとトラップします（`TrapKind::NumericRuntime`）。`Arc` の増加は `atomicrmw add ... monotonic`、減少は `atomicrmw sub ... release` で、0 にしたタスクは値とブロックを壊す前に `fence acquire` を置きます。`Arc.try_unwrap` は `cmpxchg 1 → 0`（monotonic）の後に `fence acquire`、`Arc.upgrade` は強い数が 0 でない間 `cmpxchg n → n + 1`（成功は acquire）を繰り返し、計数の読み出しは `load atomic ... monotonic` です。`Rc` は atomic 命令を使いません。既定の wasm32 では LLVM が atomic 命令を通常の命令へ下げます。
Rc／Arc を使わないプログラムの IR は変わりません。検証は `cargo test --locked --test rc` と `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri rc`（`--wasm-feature threads` の WASM を `createThreadPool` で実行する検査を含む）、`TSUZURI_TSAN=1` での同じ suite です。

### Atomic／Mutex／Task.scope（F10）

`Atomic<'a>`（`record Atomic<'a> { cell: ['a] }`）と `Mutex<'a>`（`{ cell: [(i32u * 'a)] }`）は `stdlib.rs` の不透明な標準 record（`opaque_record`、opt-in の `OPT_IN`）で、`std/Atomic.tz`・`std/Mutex.tz` は宣言だけを持ちます。セルは配列のバッファに置くので、record のムーブでセルのアドレスは変わらず、共有借用の間は動きません。関数は `Builtin::AtomicCreate`〜`AtomicIntoInner`、`MutexCreate`／`MutexWith`／`MutexIntoInner`、`TaskScope`（`sync_scheme` が型と制約を持つ。名前は `Atomic.*`、`Mutex.with_lock`、`Task.scope`）です。組み込みの型クラス `Sync` と `AtomicValue` は `BUILTIN_CLASSES`（33 個）に加わり、`intrinsic` が `Type::can_sync` と整数・bool の判定を引きます。利用者の `instance` は `E1016`、`dyn` の印は `E1028` です。`Type::is_shared_cell` は `Atomic`／`Mutex` を判定し、`can_capture` が関数値への捕捉を拒みます（`E1005`）。`Type::has_interior_mutability` は共有借用から書き換わる型を判定する関数で、PR01 の計画中の `Type::is_frozen` がその否定を使います（実装済みの呼び出し元はまだなく、単体テストだけが使います）。
生成（`src/llvm_sync.rs`）: `Atomic` の操作は `atomicrmw`（`add`・`sub`・`and`・`or`・`xor`・`xchg`）、`cmpxchg`、`load atomic`、`store atomic` の `seq_cst` 1 命令で、`bool` は 1 バイトに広げます。`compare_exchange` は `cmpxchg` の結果から `Result` を組み立てます。`Mutex` のセルは `(i32u * T)` 1 要素の配列で、先頭の語がロック、2 つ目が値です。`Mutex.with_lock` は `@tsuzuri_mutex_lock(ptr cell)` の戻り値が 0 でなければ標準のトラップ経路（`TrapKind::Assert`）に入り、0 ならコールバックを呼んで `@tsuzuri_mutex_unlock` します。`Task.scope`（`src/llvm_parallel.rs` の `parallel_scope`）は、1 つの子どもを 1 つのグループの要素として `tsuzuri_task_parallel` に投入し、共有借用と添字をコールバックに渡して結果を添字の位置へ書きます。借用された一時値（`Task.scope (ref 5) ...`）は、呼び出しの前後でスロットに置いて解放します。`src/ownership.rs` は `Task.scope` の共有引数と `Parallel.map_ref` などの入力配列について、借用を持つ環境を `E1013` で拒否し、結果の借用も拒否します（`shares_borrowed_environment`。名前を付けた値は、その値が借用を持つか（loan の親が空か）で判定し、一時値の `BorrowOperand` は、一時値自身が借用を 1 つでも持てば拒否します）。`Mutex.with_lock` は、結果の型が借用を持ちうるとき（`locked_result` が呼び出しの型から求めます。クロージャ変換後は、組み込みを呼ぶ生成関数も同じに扱います）、`callback` の結果が借用を持たないと証明できなければ（`closed_result`、並列の callback と同じ証明）`E1013` で拒否し、そのときは引数の一部だけの適用と関数値化も `E1013` です。`Channel.send` の要素も同じ作りで検査します（`builtin_called` がクロージャ変換後の生成関数を含めて組み込みを求め、`sent_item` が要素の型を返します）。`Sender<'a>` は `{ block: i64 }` で、`carries_loans` には要素の型が見えないので、要素の型が借用を持ちうるとき（関数値を含むとき）に、直接呼びの要素の `Value` が loan を 1 つでも持てば（引数の借用も、`ref` した局所値の借用も同じに数えます）`E1013` です。生成関数の引数は呼び出し側で検査され、部分適用と関数値化は要素の型が借用を持ちうるとき `E1013` です。`recv` の呼び出しの結果は、入れるときに検査済みでチャンネルから移された値なので、loan を持たない値にします。`Mutex.create` は、`Mutex<'a>` の型が `'a` を持つ配列なので、借用が通常の loan としてロックの値に残ります。
ドライバー（`src/llvm.rs` のヘッダーの組み立て）: IR が `@tsuzuri_mutex_lock(` を含むと、ネイティブと threads では宣言（`tsuzuri_mutex_lock`・`unlock`・`parallel_ok`）を足し、並列がある場合は `@tsuzuri_task_parallel` と `_results` の呼び出しを `sync.ll` のラッパー（`@tz.mutex.parallel`・`@tz.mutex.parallel_results`。ロック中なら `llvm.trap`）へ付け替えます。既定の wasm32 は `sync-wasm.ll` を連結します（ロックはグローバル変数 `@tsuzuri.mutex.held` で、`task-wasm.ll` は変わりません）。`Mutex` を使わないプログラムの IR は、バイト単位で変わりません。`--freestanding` の `Mutex` と `Channel` は `E2000` です（ドライバーが IR の `@tsuzuri_mutex_lock(` と `@tsuzuri_channel_send(` の宣言を見ます）。検証は `cargo test --locked --test concurrency`、`cargo build --release --locked && node tests/features.mjs target/release/tsuzuri concurrency`（native と wasm32、`-O0`／`-O3`、`TSUZURI_TSAN=1`・`TSUZURI_ASAN=1` を含む）、`tests/sync_runtime.c`（`tests/tasks.mjs` が各最適化レベルと TSan／ASan でビルドして実行）、`tests/trap_return_sync.mjs`、`tests/wasm_threads.mjs`（本物の Worker での `Atomic`・`Mutex`・`Task.scope`・`Channel`）です。

### Channel（F10 Phase 2）

`Channel.Sender<'a>` と `Channel.Receiver<'a>` は `std/Channel.tz` の不透明な標準 record（`{ block: i64 }`、`opaque_record`、opt-in の `OPT_IN`、`RESERVED_MODULES` は 56 個）で、`Drop` の instance も `std/Channel.tz` が書きます。`polymorph::validate_drop_instance` は、std のモジュールが自分で宣言した record・union（名前が `<モジュール>.` で始まるもの）には `Drop` の instance を許し、利用者の std 型への instance は `E1016` のままです。組み込み関数は `Builtin::ChannelBounded`・`ChannelSend`・`ChannelRecv`・`ChannelCloneSender` と、std 専用で `E1022` になる `ChannelCloseSender`・`ChannelCloseReceiver`（drop 本体から呼ばれ、`ref` で受けます。drop 本体は排他参照を渡せないためです）で、型は `sync_scheme` が持ちます。
ブロックの配置は全ターゲットで共通です: `capacity`・`item_size`・`head`・`count`・`senders`・`receivers`・`owners`・予約の各 8 バイト、待ち用の 2 語、で 80 バイトの先頭（`CHANNEL_*` 定数）と、要素のリングバッファです。IR（`llvm_sync.rs` の `channel_*`）が `@tz.alloc` でブロックを確保して初期化し（`AllocationSize` の検査、容量 0 以下は assert のトラップ）、両端の値を `mark_live` で生きた印にして返します（利用者の `Drop` を持つ値の暗黙 drop は、この印で決まります）。`send`・`recv` は要素をスタックの 1 つのスロット（`spill`・`slot`）に置き、ランタイム関数（`tsuzuri_channel_send`・`recv`・`clone_sender`・`close`）へそのアドレスを渡します。`send` が 1 を返すと、スロットの要素を `Error` に入れて呼び出し元へ返します。操作の前に `tsuzuri_mutex_wait_ok()` が 0 を返すと（ロック中）、assert のトラップです。ランタイムの結果（0 成功、1 閉じている、2 デッドロック）の 2 は assert のトラップです。`close` は最後の所有者に 1 を返し、その所有者が残りの要素を `array_loop` と `drop_value` で drop して `@tz.free` します。
ネイティブの待ち（`task.c`）: `tz_task_pool` の mutex 1 つが、全チャンネルの操作と次の数え上げを守ります。`busy` は Tsuzuri のコードを走らせているスレッド（アイテムを取ったワーカー、`submit` の中で結合を待っていないスレッド、チャンネルの待ちから起こされたスレッド、チャンネル操作の間の外部スレッド）で、`waiters` は待ちの一覧、`idle` は仕事を取っていないワーカー（待っているものと、始まっているもの）です。待ちスレッドは、スタック上の `tz_waiter`（スレッドごとの条件変数を持つ）を一覧へ入れ、`busy` から外れます。起こす側が、起こすスレッドを `busy` へ戻してから（起こされたスレッドが自分で戻すと、その間に別のスレッドが「全員待ち」と誤判定します）、その 1 つだけを起こします。判定（`tz_channel_check_deadlock`）は、`busy` が 0 になる、または待ちが増えるたびに、`busy == 0`・待ちあり・結合が終わる見込みのグループなし・（空きワーカーがいるなら）未着手のアイテムなし、を見て、成り立てば待ちを全員起こします。手伝い（`tz_task_help_one`）は、空きワーカーがいないとき、未着手のアイテムを 1 つ、自分のスタックの上で走らせます（深さ 16 まで）。空きがいるときに手伝うと、生産者が消費者を自分の上で走らせ、生産者の続きが消費者の終わりを待つ形になるためです。1 CPU とアイテム 1 つの経路は、数え上げなしで逐次に走るので、待ちは未着手の仕事のない判定になります。
threads の待ち（`task-wasm-threads.c`）: 外部スレッドはなく、スレッド数は固定（メイン + ワーカー、プールの開始前は 1）なので、数え上げは「進展ごとのリセット」です。手伝い（`help_one`）は、`idle`（仕事を取っていないワーカー）が 0 のときだけ、最も古いグループの未着手のアイテムを自分のスタックの上で走らせます。native と同じく、グループは出した順（古いものが先、`groups` と `groups_tail`）に並べ、`submit` は、グループを公開する前に最初のアイテム（添字 0）を自分のものにし（`next` の初期値は 1）、そのあと自分のグループの添字を取り尽くします。結合を待つあいだは、ほかのグループのアイテムを走らせません。ほかのグループのアイテムを結合のフレームの上に載せると、そのアイテムが、結合のあとのコード（同じスタックの下）が作るものを待ちうるからです。外側の `Task.parallel [生産者, 消費者]` で、生産者の中の入れ子のグループを待つスレッドが消費者を取ると、消費者は、そのスレッドが結合のあとに送るトークンを待ち、全員が待って、誤った判定になりました。最初のアイテムを公開の前に取るので、公開から最初のアイテムを取るまでの間に、ワーカーがアイテムをすべて取り、待ち始めたワーカーが最後の段を自分の下の段の上に積んで、最少のスレッド数のパイプラインが詰まる窓もありません（結合を待つスレッドを `idle` に数える必要がありません）。`blocked`（最後の進展の後に待ちを登録したスレッド）と `channel_waiters` を `queue_mutex` で守り、チャンネルの状態を変える操作（待っている者がいるとき）、グループの追加、グループの最後のアイテムの完了、競合した `Mutex` の解放だけが `progress()` で両方を 0 に戻して `epoch` を進めます。眠っているスレッドが待つのは、この 4 つのどれかなので、グループの途中のアイテムの完了は誰も起こしません（1 つのグループで `epoch` が動くのは、追加と、worker が走らせた最後の完了の 2 回までです。投入したスレッド自身が最後なら、完了では動きません）。`progress()` は `epoch` を lock の中で進め、通知（`memory.atomic.notify`）は `unlock_queue()` が lock を放してから送ります。起こされたスレッドが、通知元が持つ lock を待たないようにするためです。眠るスレッド（アイドルのワーカー、結合を待つスレッド、チャンネルの待ち）は、登録したときの `epoch` が動くまで `sleep_until_progress` で眠り続けるので、遅れて届いた通知や別の理由の通知で起きても、登録し直さず（二重に数えず）眠り直します。host の `fail` は `failed` を立ててから `epoch` を進めるので、眠る前にも `check_failed()` を行います。`fail` が済んだあとに `epoch` を読んだスレッドは、そのフラグを見てトラップし、二度と動かない `epoch` で眠り続けません（それ以前に読んだスレッドは、`epoch` が動いているので待ちがすぐに戻ります）。アイドルのワーカーは `join_one` で、どのグループにもアイテムが残っていないと判定した lock の中で、結合を待つスレッドは `submit` の待ちの loop で、自分のグループがまだ終わっていないと判定した lock の中で、そのまま登録します。起こされたスレッドは条件を見直し、まだ進めなければ登録し直します。`blocked` がスレッド数に達し、チャンネルの待ちを含むとき、最後に登録したスレッドが `verdicts` を進め、待ちスレッドはそれを見て `status 2` を返します。スレッドごとの状態（いまのロック、手伝いの深さ）は、`address_space(1)` の変数（インスタンスごとの WebAssembly のグローバル）で、Worker ごとに独立します。`__wasm_init_tls` の領域は使いません。`Mutex` の待ちは `epoch` で眠るので、ホストの `fail()`（`epoch` を進めて起こす）で、失敗したプールでも待ち続けません。`ThreadState`（28 バイト）の配置は変えず、ホストの JS は変わりません。
検証: `tests/channel_runtime.c`（C のランタイムを条件変数だけで駆動し、1〜4 スレッドと TSan／ASan で、リング、閉じる、待ちと起こし、判定、手伝い、`Mutex` 中の拒否を確かめます）、`tests/features.mjs` の `channel` スイート（native と wasm32、`-O0`／`-O3`、トラップ、残存確保数 0）、`tests/fixtures/channel_threads` を 1〜4 スレッドで走らせる `tests/tasks.mjs`、`tests/wasm_threads.mjs` と `tests/bindings_threads.mjs`（本物の Worker）、`tests/trap_return_sync.mjs`、`tests/concurrency.rs` です。
### 正規表現と Unicode の表（D09）

`Regex` は `std/Regex.tz` の Tsuzuri ソースだけで書いた Pike VM で、コンパイラに専用の型・構文はありません（`stdlib.rs` の予約名と不透明レコードへの登録、`Type::is_noncopy_record` による非 Copy 化だけ）。
`compile` は明示的なスタックで構文木（1 本の `Vec<i64>` に 6 語ずつの節点と子の列）を作り、節点ごとの命令数を飽和計算して上限を検査してから、`def rec emit` が 3 語 1 命令の `[i64]` を書きます。`[...]` は同じエスケープを 1 回だけ読み（2 回目からは表を読まず何も足さない）、閉じた後に併合・畳み込み・否定した区間を `add_class` が class の区間の合計の上限と比べます。照合は `search` と `add_thread` が呼び出しごとに確保する 1 本の `[i64]` の作業領域（手数カウンター、2 本のスレッドリストの dense／sparse 集合と捕捉表、作業用と最良の捕捉、`add_thread` の明示的スタック）を排他スライスで更新し、照合の内側では確保しません（`tests/features.mjs` の `regex` スイートが IR で検査）。
Unicode の表は `scripts/generate-unicode.mjs` が UCD 17.0.0 の入力（SHA-256 を固定）から生成する `src/runtime/unicode.ll` の `internal` 定数（`[N x i32]`、19 個）と、表番号で分岐する `@tz.unicode.length`／`@tz.unicode.entry`（範囲外はトラップ）です。
表は一般カテゴリー・正準結合クラス・書記素（`Grapheme_Cluster_Break`、`Extended_Pictographic`、`InCB`）・単語（`Word_Break`、`Extended_Pictographic`）の連続区間（`start << shift | value`）、二値 property の閉区間、分解（キー、`(符号位置または pool の位置) << 6 | 長さ << 1 | 互換`、pool）、一次合成の 3 つ組、大文字小文字の対応の連続した組（`start, count, stride, delta`）、`SpecialCasing.txt` と `CaseFolding.txt` の状態 F の完全な対応（`code << 2 | 種類` と pool）です。ハングル音節は表に載せず、`std/Unicode.tz` が算術で分解・合成し、書記素の LV と LVT を区別します。単語境界の WB6・WB7b・WB12 の先読み（Extend・Format・ZWJ の続きの次のスカラー）は後ろからの 1 回の走査で前もって求め、`tests/unicode.rs` は分割と大小変換の関数（`word_breaks`・`grapheme_breaks`・`converted`）の型付き IR にループの入れ子が無いことを検査します。std 専用の組み込み `Unicode.__table_length :: i64 -> i64` と `Unicode.__table_entry :: i64 -> i64 -> i64` がこれを呼び、std の `Unicode` と `Regex` 以外からの参照は `E1022` です。
`emit_target` は生成 IR に `@tz.unicode.` が現れるときだけ `unicode.ll` を連結するので、使わないプログラムの IR と WASM の import は変わりません。
組み込みの関数と `@tz.unicode.length`／`@tz.unicode.entry` は `alwaysinline` で、表番号が定数の呼び出し位置では `switch` が 1 つの表の読み出しに畳まれ、`-O3` では読まれない表を LLVM が削除します。
そのため std は表番号を実行時の値（捕捉した変数など）として渡す経路を作らず、表番号を引数に取る補助関数（`rank`・`run_map` など）は呼び出し位置ごとに定数で呼びます（大小変換の種類のように実行時の値で表を選ぶ箇所も、各分岐の中で定数の表番号を書きます）。trap の種類は `BoundsCheck` です。
`std/Unicode.tz` の正規化・分割・大文字小文字の変換は、入力をスカラーの `[i64]` に復号してから処理し、`string` と `utf8string` に符号化し直します（表の検索は二分探索）。
検証は `cargo test --locked --test regex`・`--test unicode` と `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri regex`・`unicode` です。`unicode` の期待値は、UCD の `GraphemeBreakTest.txt`・`WordBreakTest.txt` の全件と `NormalizationTest.txt` の抜粋（`tests/unicode-ucd.mjs`。`node tests/unicode-cases.mjs --write <UCD>` で再生成し、V8 が同じファイルの全行と一致することも確かめる）、全スカラーのカテゴリー・正規化・大文字小文字の変換（V8）、`CaseFolding.txt` の畳み込み、V8 の `Intl.Segmenter` と無作為な列です。後者のケースは `tests/regex-cases.mjs`（`--write` で `tests/fixtures/regex/Cases.tz` を再生成）が V8 の `u` フラグの正規表現で期待値を計算し、`\p{...}` の全スカラーの区間と `iu` の畳み込みの軌道を V8 と照合します。

### Monoid（F15）

`Monoid<'m>` は `std/Monoid.tt` のクラスで、常時ロードされます（`Array.tz`・`Parallel.tz` が使うので opt-in にしません。クラスの修飾名は `Monoid.Monoid`）。整数向けの `Monoids.Sum`・`Monoids.MaxOf` などは opt-in の `std/Monoids.tz` で、`OPT_IN` の名前は `["Monoids"]`、`RESERVED_MODULES` は 56 個です。`Array.fold_monoid` と `Parallel.fold`（`Parallel.reduce (Monoid.empty ()) (\left right -> Monoid.combine left right)`）は通常の Tsuzuri 関数で、専用の `Builtin`・ランタイム関数・WASM の import を持たず、`Parallel.reduce` の実行時の規則（チャンク数・結合順・WASM の逐次経路）をそのまま使います。
`E1039`（kind `monoid_float_instance`）は `polymorph::ClassEnv::normalize` が、`Monoid<T>` の制約で `T` が浮動小数点の数を含む（`Type::holds_float`。`TypeContext::reaches` でフィールド・payload・要素・参照先を辿り、`Binary`・`Decimal` か浮動小数点のレーンの `Simd` に届く。`reaches` が辿らない排他スライスの view `ArrayView` の要素は、見つけた view ごとに一度ずつ辿り直す）ときに、制約の位置で出します。型変数を含む `T` も、浮動小数点がすでに届けば同じです。型変数からしか浮動小数点が来ない `T` は、instance の文脈へ置き換わって制約が残らないので、`Specializer::lower` が `Method` の受け手を解決するところでもう一度調べ、その式の位置で出します。`validate_instance` ではなく `normalize` に置くのは、instance が見つかった制約（`Monoid<Monoids.Sum<f64>>`）が `validate_instance` を通らず、文脈の制約（`Integer<f64>`）へ進むためです。法則（結合律・単位元）は検査せず、`tests/fixtures/monoid` の `Gen` の性質テストと、非可換な `Affine` を含む E2E suite `monoid` が std の instance を確かめます。

### 行列と N 次元配列（C11）

`Matrix`・`MatrixView`・`Tensor` は `std/Matrix.tz`・`std/MatrixView.tz`・`std/Tensor.tz` の通常の Tsuzuri ソースだけで書かれ、専用の `Builtin`・`Type` の variant・ランタイム関数・WASM の import を持ちません。`record Matrix<'a> { rows: i64, cols: i64, data: ['a] }` は行優先の 1 本の `%tz.array` で、コンパイラ側の登録は `stdlib.rs` の `SOURCES`・`RESERVED_MODULES`・`OPT_IN`（`MatrixView` は `Matrix`、`Tensor` は `Matrix` と `MatrixView` を `uses` に持つ）・`opaque_record`（`Matrix.Matrix`・`MatrixView.MatrixView`・`MatrixView.Mut`・`Tensor.Tensor`・`Tensor.View`）と、`Type::is_noncopy_record`（`Matrix.Matrix`・`Tensor.Tensor`・`Tensor.View`。`MatrixView` は共有借用だけの Copy の窓、`Mut` は排他スライスを持つので既に非 Copy）だけです。`opaque_record` が構築・フィールド参照・パターン・更新を `E1022` にします。`is_noncopy_record` を登録しないと、`'a` が Copy のとき `data: ['a]` も Copy になり、`let n = m` が長さに比例する暗黙の複製になります。opt-in（D-40）なので、これらを名指ししないプログラムの型検査と IR は追加前と同じです（fixture・例・ベンチマークの IR を native と wasm32、`-O0` と `-O3` でバイト比較して確かめました）。
不変条件（`rows >= 0`、`cols >= 0`、`rows * cols` が `i64` に収まる、`data.length == rows * cols`）は構築する関数だけが検査・保存するので、`at`・`row`・`mul` の添字計算 `i * cols + j` は溢れません。`checked_count` は乗算の前に除算で溢れを判定します。
`mul` の実装 `multiply_rows` は、出力を行ごとに `k`・`j` の二重ループで更新する i-k-j 順で、`out(i, j)` は `out(i, j) + left(i, k) * right(k, j)` だけで更新され `k` が昇順に進みます。出力要素ごとの演算の列は i-j-k の素朴な和と同じで（積と和は別の命令 `fmul` と `fadd`、整数は `mul` と `add`）、変わるのは別々の出力要素を訪れる順序だけです。IR に `llvm.fmuladd`・`fast`・`contract`・`reassoc` は出ず、`multiply_rows` に `llvm.fma` も出ません。`-O3` の LLVM もフラグのない浮動小数点の加算を並べ替えないので、`k` 方向の再結合は起きません。`left` の行は先にスライスで切り出してあります。要素ごとに `left[base + k]` を検査する形では、LLVM がベクトル化したループに配列の長さから作る重なりの検査を付け、out-of-line の関数ではその検査に通らない配置で約半分の速さになりました（インライン化されると検査は消える）。スライスの長さが `inner` だと証明できる形では検査が正確になり、配置によらず同じ速さです。
`mul_fma` の `fma_rows` は同じループで `Math.fma a source[j] row[j]` を 1 回ずつ使い、`Array.dot_fma` と同じ `fma(a, b, total)` の順序です。IR の `fmuladd`・`contract` はなく、`Math.fma` は AArch64 の native（ビルド機が AArch64 のときの `--target native`、または AArch64 の triple。`Platform::hardware_fma`）では `llvm.fma`、それ以外の native と wasm32 では `tz_soft_fma` の呼び出しです（`src/llvm_math.rs` の方針で、C11 は変えていません。ソフトウェアのルーチンは Apple M1 Max で 1 積和あたり約 1.6 µs で、`mul_fma` の 128 × 128 × 128 の積が約 3.4 s、ハードウェアでは約 0.34 ms です。ビットは同じです）。テストは、`multiply_rows` に `llvm.fma` も `tz_soft_fma` も出ないことを検査し、`mul_fma` の呼び出し先はどちらでも受け付けます（ネイティブの IR は `llvm.fma` か `tz_soft_fma` のどちらか一方、wasm32 の IR は常に `tz_soft_fma`）。`mul_parallel`・`mul_fma_parallel` は、出力の行を `Parallel.for_each_chunk` で分けます。1 チャンクは約 $2^{20}$ 回の積和になる行数（`max(1, 1048576 / (inner * cols))` 行）で、形だけで決まります。コールバックは借用を捕捉できないので、2 チャンク以上では入力を `Arc` に複製して共有し、1 チャンクならコピーせずに `multiply_rows` を直接呼びます。
`MatrixView` は `{ data: ref { r } ['a], offset, rows, cols, row_stride, col_stride }`、`Mut` は `data: ref mut { r } ['a..]` を持つ借用のレコードです（A09・A12 の region 規則）。窓の関数は region パラメーター `{ r }` を明示し、`transpose`・`sub`・`row`・`col` は同じ `data` を別の `(offset, strides)` で指し直します。窓の要素が `data` の中にあるという不変条件は `strided` が `last_index`（最後の要素の添字で、要素のない窓は `-1`。2 つの乗算は先に除算で溢れを判定し、足し算の結果を `data.length` と比べるので、`i64` の最大値の添字も `data` の外として弾き、`+ 1` で折り返すことはありません）で検査し、操作が保ちます。`mul` は形を `assert` してから窓を複製します。`fold`・`fill`・`copy_from`・`map_in_place`（と `Tensor.fold`）は、要素のない窓では他の軸を走査せずに戻ります（長さ 2^62 の軸と長さ 0 の軸を持つ窓が有効なので、走査すると `-O0` では戻らず、`-O3` では空のループが消えて最適化水準で挙動が分かれます）。`Mut` は `of_array_mut`・`sub_mut`・`transpose_mut` からだけ作れ、配置が行優先かその転置になるので異なる要素が同じ場所を指しません。`sub_mut` は、`ref mut view.data[…]` を作る前にストライドを局所変数へ取り出します（書き込み先の借用と同じ式で `view` のフィールドを読むと `E1014` です）。局所の所有者の窓を返すと `E1013`、窓が生きている間の所有者の move・置換や、`Mut` が生きている間の元の配列の読み書きは `E1014` です。
`Tensor` は `record Tensor<'a> { shape: [i64], data: ['a] }` と `record View<'a> { r } { data: ref { r } ['a], offset: i64, shape: [i64], strides: [i64] }` です。軸は 16 本までで、`volume` が長さの積を、長さが 0 の軸を除いて溢れなく数えます。`fold` と `to_tensor` の `gather` は、最も内側の軸を関数呼び出しなしのループで処理します。窓は形とストライドの配列を所有するので非 Copy で、API は `ref` で受け取ります。
検証は `cargo test --locked --test matrix --test matrix_views --test tensor` と `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri matrix`・`matrix_view`・`tensor` です。期待値は `tests/matrix-cases.mjs`（JavaScript の number（binary32 は演算ごとの `Math.fround`）と BigInt。FMA は BigInt で厳密に計算して 1 回だけ丸める）、`tests/matrix-view-cases.mjs`・`tests/tensor-cases.mjs`（窓を、ストライドではなく添字の写像のクロージャで独立にモデル化する）が計算します。suite `matrix` は、並列の積が WASM threads（ワーカー 3 つ）でも `mul` と同じビットになることと、CPU 数を 1 と 4 に上書きした native の実行も確かめます。suite `matrix`・`matrix_view`・`tensor` は、wasm32 を `--trap-info` 付きでもビルドし、`src/runtime/trap-boundary.mjs` の `createBoundary` で各トラップの種類が `assertion failed`（確保や要素の読み出しの前の検査）であることも確かめます（`alloc_size` だけ `allocation size overflow`）。空の窓の巨大な軸の確認（suite の `bounded`）は、戻らないことが失敗なので、native では 1 件ずつ別のプロセスで、WASM では子の Node プロセスで実行し、どちらも 10 秒の SIGKILL のタイマーを付けます（`tsuzuri run` 経由にすると、タイマーが切るのは `tsuzuri` だけで、コンパイルした `program` が残ります）。

### 共有配列ビュー

`ref [T]` は非所有の配列記述子 `%tz.array = { ptr, i64 }`（ポインタと要素数）として表現され、値のサイズは 16 バイトです。
一方、可変長ベクタ `Vec<T>` は `%tz.vec = { ptr, i64, i64 }`（データポインタ、要素数、確保容量）として表現され、構造体サイズは 32 バイトとなります。
`Vec` は常に非 Copy ですが、クロージャ環境の複製時などにおける内部的な clone は、複製した関数が元と同じに振る舞うように、確保容量を維持したまま独立したバッファとして安全に複製されます。明示的な `Vec.clone` だけは、確保容量を長さにそろえます（`new List<T>(list)` と同じです。`clone_vector_exactly`）。デストラクタ（drop）は有効要素数（length）の範囲内の要素のみを解放します。
容量拡張に伴う再確保はネイティブでは `realloc`、WASM では、ヒープの終端にあるブロックはその場で伸ばし、隣接する空きブロックは吸収を試み、不可能な場合に新規確保・`memory.copy`・旧領域解放を行います。
std の `Vec` モジュール（`std/Vec.tz`）は、.NET の `List<T>` のメンバーを、`Vec` を最後の引数にして実装します（F# の `Array` や `List` と同じく、`|>` と `>>` でつなげられるため）。`push`・`pop`・`set`・`swap`・`get`・`at`・`reserve`・`truncate`・`clear` などは従来どおりコンパイラ組み込み（`src/check.rs` の `Builtin`、`src/llvm_bulk.rs` の `vector_builtin`）で、これらも `Vec` を最後に受け取ります。まとめて要素を動かす操作と容量の操作のために、利用者には見えない 4 つの組み込みがあり、std の `Vec` だけが使います。`Vec.__view :: ref mut Vec<'a> -> ref mut ['a..]` は、初期化済みの要素を排他スライスとして貸し、容量は変えません。これにより `Array` のその場アルゴリズム（`sort_in_place`、`swap_in`、`write`）が `Vec` のバッファ上で動きます。`Vec.__set_capacity :: i64 -> Vec<'a> -> Vec<'a>` は容量をちょうど指定値にします（長さより小さい値はトラップです）。`Vec.__insert :: i64 -> 'a -> Vec<'a> -> Vec<'a>` は 1 要素を挿入し、`Vec.__splice :: i64 -> i64 -> ['a] -> Vec<'a> -> Vec<'a>` は「位置から `removed` 個を捨て、所有する配列の要素をそこへ移し入れる」操作です。後ろの要素のずらしは `llvm.memmove`、配列からの複製は `llvm.memcpy` で、native と wasm32 のどちらでも libc の `memmove` や `memcpy` に依存しません。これらの容量の拡張は、.NET の規則（空なら 4、以降は 2 倍、足りなければ必要な個数）を 1 か所（`grow_vector`）で実装しています。`push` と `reserve` は従来どおり、必要な個数に届くまで 2 倍にします。
NULL ポインタや要素数 0 のケースはヘッダー読み出しよりも前に早期判定され、WASM のメモリ上限（デフォルト 16 MiB）は厳格に維持されます。
`ref mut [T]` およびその他の参照型は引き続き生のポインタ `ptr` として渡されます。共通のメモリレイアウト、クロージャ捕捉、関数引数、および戻り値もこの表現に統一されています。
配列スライス `Slice` は、元配列への借用を確立した後に範囲式の検査を行い、LLVM コード生成ではすべての境界検査が完了した後にのみ GEP（ポインタ計算）と長さの差分計算を実行します。
共有配列参照に対する非消費的な参照外しは配列ビューを読み出し、所有値としての参照外しは従来の配列ディープコピーを実行します。
要素への借用はビューから要素のアドレスを直接算出します。パターンマッチにおいて構造体全体の射影が必要な場合に限りエントリーブロックの一時記述子へ保存されますが、ユーザーコードに対してこの記述子への可変アクセスが公開されることはありません。スライスオブジェクト自体は clone や drop 時にバッファ操作を行いません。

### 排他配列ビュー

排他スライス `ref mut [T..]` は型検査器の中では `Type::Reference(ArrayView(T), true)` であり、`ArrayView` は排他参照の参照先にだけ現れます。
表現の判定は `Type::slice_element` に集約されており、共有スライスと同じ `%tz.array` 記述子（16 バイト）を値として受け渡します。長さは固定なので、`ref mut [T]` のような記述子の置き場所へのポインタは不要です。単相化のシンボルでは `view[T]` と正規化されます。
排他スライスはすべて `TypedExprKind::Slice` として型付けされます。`ref mut values` や `ref mut [T]` から配列全体のビューへの変換（`src/check.rs` の `whole_view`）、ビューの貸し直しも同じ形になるため、所有権検査とコード生成の経路は一つです。
所有権検査は二相です。`src/ownership.rs` は範囲式を評価している間だけ元の place に共有の guard loan を置き（範囲の中の読み取りは許し、書き込みは拒否します）、評価後に排他 loan を作ります。`Array.split_at_mut` の二つの結果は実引数の貸し直しの loan を一つ共有し、範囲を区別しません。
スタックフレーム上の局所配列から排他スライスを作るときは、記述子を読む前に `own_heap_storage` が要素領域をヒープへ移します（`Array.write` が旧要素を解放するため、フレーム上の要素を `tz.free` に渡さないようにします）。参照外しを元とするスライスは移送しません。
`Array.write`、`Array.swap_in`、`Array.split_at_mut` は組み込みで、`checked_element_pointer` の境界検査（`TrapKind::BoundsCheck`）の後に旧要素を解放して格納します。スライスは最後の引数です（`Array.write 添字 値 スライス`、`Array.swap_in 添字 添字 スライス`、`Array.split_at_mut 添字 スライス`）。スライスより前の引数の式は、スライスを貸し出す前に評価し終わるので、同じ配列を読めます（`Array.write 0 (values[1] * 10) values`）。`llvm_facts::scalar_slice_call` は、この 2 つの書き込み（`write`、`swap_in`）の呼び出しの最後の IR 引数からスライスの記述子を読みます。`Array.sort_in_place` は `std/Array.tz` の関数で、挿入整列と SymMerge による作業領域なしの安定整列です。
`Parallel.for_each_chunk` は `src/llvm_parallel.rs` の `parallel_chunks` が `ceil(length / size)` 個のチャンクへ分け、各 worker へ互いに素な記述子を渡します。

### 固定長配列

`[T; N]` は型検査器の中では `Type::FixedArray(element, length)` で、長さは `Type::Length(n)` または長さパラメーター `Type::Variable("#N")` です。長さを型として持つので、型置換・単一化・単相化・別名展開は型変数と同じ経路を通ります（`Type` の大きさは 4 ワードのままです）。構文木では `TypeExprKind::FixedArray` と `TypeExprKind::Length` で、`const N: i64` は型パラメーター名 `#N` として登録されます。
長さの解決は `src/check.rs` の `resolve_length` に集約されています。名前の長さは、整数リテラルを初期化式に持つ `i64` 定数（`Names::length_constants`）、型宣言の長さパラメーター、関数の暗黙の長さパラメーターの順に解決し、1024（`MAX_FIXED_ARRAY_LENGTH`）を超える長さは `E1010` です。
LLVM では `[N x T]` の値として表現し、タプルと同じくフレーム上へ直接置きます。`llvm_frame::stack_size` と `Layouts::size` は要素サイズの `N` 倍で見積もり、単相化のシンボルでは `fixed[N,T]` と正規化されます。
添字は `fixed_element_pointer` が GEP で要素のアドレスを求め、`N` 未満の整数リテラルの添字では境界検査の分岐を出力しません。clone と drop は非 Copy 要素のときだけ要素ループ（`array_loop`）を生成し、名前付きの値から `ref [T]` への変換（`fixed_array_view`）は `%tz.array` 記述子を作るだけで要素を複製しません。
`FixedArray.init` の長さは `src/polymorph.rs` の `FamilyKind::FixedArrayElement` が期待型から決めます。直接の呼び出しは `fill_fixed_array` がフレーム上の値へ初期化関数を昇順に適用するインライン展開になり、関数値として使う場合は結果の配列型ごとの組み込みラッパーが同じ展開を行います。
公開 ABI では、スカラー型の固定長配列フィールドを C 構造体の配列メンバー `T name[N]`（LLVM では `[N x abi]`）へ正規化します。ISO C に長さ 0 の配列はないので、`[T; 0]` のフィールドを持つレコードは公開できません（E1008）。

### dyn 値

`dyn C` は型検査器の中では葉の `Type::Dyn(Box<DynType>)` で、`DynType` はクラスの正規名（`Class::name`）の列と印 `copy`・`send`、region を持つかどうか（`borrowed`）を持ちます。型の性質（`is_copy`、`can_send`、`carries_loans` など）はこの印だけで決まります。
構文木の `TypeExprKind::Dyn` は parser が `Program::dyn_types` にも記録し、`Classes::collect` の最後に dyn 互換規則（`Classes::dyn_slots`）を出現順に検査して `E1028` を報告します。
`Classes::dyn_instances` は、書かれた互換な dyn 型ごとにディスパッチする各クラス `X` の `X<dyn ...>` インスタンスを合成し、本体を `ExprKind::DynDispatch { slot, slots }` にして通常のインスタンスの経路へ流します。このため制約付きジェネリック関数は `'a = dyn C` で一度だけ具体化されます。
`Dyn.of` は特殊化で `(vtable キー, 格納型)` ごとに slot 関数を解決して `CheckedModule::vtables` に記録します。vtable キーは `DynType::vtable_key`（`send` と `borrowed` を消したもの）で、組み込みの実装は組み込みラッパー関数を slot にします。アップキャストは `dyn_layouts` が元の vtable の slot 関数を使い回した受け先の vtable を不動点まで足し、`CheckedModule::dyn_layouts` に各キーの slot 数とアップキャスト先を持ちます。
LLVM では値を `%tz.dyn = type { ptr, ptr }`（data、vtable）とし、この型は dyn 型を書いたプログラム（`CheckedModule::uses_dyn`）だけがヘッダーへ出します。data は `@tz.alloc` で `storage_layout`（64-bit の上界）の大きさを確保します。
vtable は `internal unnamed_addr constant` の `{ ptr drop, ptr clone, i64 size, i64 align, [N x ptr] slots, [U x ptr] upcasts }`（clone は `Copy` 印のないキーで null、upcasts はアップキャスト先があるときだけ）で、記号は `@"tz.vtable.{クラス}[{格納型}]"` です。
slot は関数ごとの adapter `@"tz.dyn.slot.{関数}"` を指します。adapter は `(ptr data, 残りの引数) -> 結果` の型で、受け手を共有配列なら `%tz.array` の読み出し、参照なら `ptr` のまま、値なら読み出して領域を解放してから実装を直接呼びます。そのためディスパッチ側の間接呼び出しの型はメソッドだけで決まり、WASM の `call_indirect` の型検査と一致します。
drop slot `@"tz.dyn.drop[T]"` は `drop_value(T)` と同じ drop glue（利用者の `Drop` を含む）の後に領域を解放し、clone slot `@"tz.dyn.clone[T]"` は新しい領域へ `clone_value(T)` します。値の drop と clone は共有の `@tz.dyn.drop` / `@tz.dyn.clone` が vtable を読んで呼びます。vtable・adapter・drop 関数は全関数の後に `BTreeMap` の順で出力します。

### 境界検査の省略

`ranges.rs` は関数ごとに 1 回、型付き IR をワークリストアルゴリズムで走査して配列添字に関する不変事実 `RangeFacts` を構築します（走査ノード数が 65,536 個を超える巨大な関数については解析を打ち切り、すべての境界検査を安全に残します）。
`Type::Array` に対する添字アクセスにおいて、閉じた静的規則のみによって「添字が必ず配列の有効範囲内に収まる」と証明できた場合に限り、境界検査の条件分岐コードの生成を省略します。証明規則として対象となるのは、`0 .. len - 1` および `len - 1 .. -1 .. 0` のループ（`for i = 0 to len - 1` と `for i = len - 1 downto 0` を含みます。`len` は `.length` または std の `Array.length`）、定数境界によるループ、リテラルや `new` による静的定数長の配列、ならびに `if i >= 0 && i < len` の then 分岐の内部です。
配列のローカル変数は、関数内で 1 回だけ宣言され、再代入も可変借用も行われない場合にのみ「安定（変更されない）」と判定されます。`Vec`、リスト、文字列、`ref mut [T]`、`Array.__set` などの組み込み関数呼び出し、および公開 ABI のエントリーポイントは解析対象外であり、境界検査がそのまま残されます。
境界検査を省略した場合でも、要素アクセス用の `getelementptr inbounds` および `load` 命令はそのまま出力され、この証明が `llvm.assume`、`!range`、`nsw`、`nuw` などの未定義動作を招きやすいフラグを新設することはありません（`nsw`／`nuw` は別の証明である整数の範囲証明が、`TSUZURI_RANGE_FLAGS=proven` のときだけ書きます。`llvm.assume` と `!range` は、そのモードでも書きません）。この解析は LLVM 最適化よりも前のコンパイラフロントエンドで実行されるため、`-O0`、`-O3`、ネイティブ、WASM のすべてのビルド構成で完全に同一の判断が行われ、省略された検査箇所はトラップ情報テーブルにも出力されません。残された検査箇所における `TrapKind::BoundsCheck` の種別やソース位置情報にも変化はありません。
境界検査を省略したメモリアクセスを含む整数リダクションループに対しては、`hint_loop` による `llvm.loop.unroll.enable`（ループ展開ヒント）の付与を意図的に抑制します。これは、検査が消えたループに対して過度な展開ヒントを付与すると、LLVM がベクトル化（Vectorize）よりも前にループを無理に展開してしまい、結果として実行速度が低下する現象（[実測データ](benchmarks.md#境界検査の残り方f12)）を防ぐための重要な設計です。境界検査が残る通常のループに対するヒント付与規則は変更していません。
検証は `cargo test --locked --lib ranges`、`cargo test --locked --test bounds_checks`、および `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri bounds_checks` によって厳密に行われています。

### 整数の範囲証明（PR02）

整数の `+ - *` は型の幅で折り返すので、LLVM の `nsw`／`nuw`（その向きでオーバーフローすると結果が poison）は、その向きのオーバーフローがないと証明できた命令にだけ付けられます。折り返す演算に付けると誤コンパイルで、メモリ安全性の問題になるので、証明のない演算には旗を書きません（`@checked` の中は検査付きの経路で、旗とは別です）。
`TSUZURI_RANGE_FLAGS` が出力を選びます。`off`（既定）は旗を書かず、PR02 より前と byte 単位で同じ IR です。`proven` は証明のある命令に旗を書きます。`audit` は旗を書かず、書くはずの向きごとに `llvm.{s,u}{add,sub,mul}.with.overflow` の結果が偽であることを検査し、真なら `TrapKind::Overflow` で止めます。audit を通ったプログラムは、その入力で証明が誤らなかったことを示すので、`proven` の試験用の経路です。変数は `cache::KEY_ENVIRONMENT` に入り、空は既定、ほかの値は `emit` のとき `E2000` です（`check` と `lsp` は IR を出さないので読みません）。ライブラリの入口は `llvm::emit_with_range_flags` で、環境変数を読みません。
証明は 2 種類です。ループの形から証明するもの（`llvm_control.rs`、`llvm.rs`、`llvm_compare.rs`）は、増分の前の終点の判定と `index < length` が根拠です。ステップ 1 または -1 の範囲ループのカウンター（符号付きは `nsw`、符号なしの昇順は `nuw`、符号なしの `downto` は `sub` の `nuw`）、narrow な範囲ループの i64 のカウンター（端点は型の範囲の拡張なので `nsw`、符号なしの昇順は `nuw` も）、配列・リスト・文字列・構造比較のループの添字（`nuw`）です。任意のステップのループは `with.overflow` の intrinsic のままで旗を付けません。
区間から証明するもの（`interval.rs`、`ranges_intervals.rs`）は、`RangeFacts` が式の取りうる値を `[low, high]`（端点は `i128`）で持ち、64-bit 以下の整数型の `+ - *` について、オペランドを符号付きと符号なしの数として読んだ数学的な結果が、その読みの範囲に収まるときだけ `nsw`／`nuw` を返します（読みが切り替わる点をまたぐ区間は全域として読み、結果は 2 つの読みの像の共通部分です）。区間は、リテラル、型の範囲、`as`、`&`・`%`・`/`・`~`・負号・定数のシフト、`Array.length` と `.length`（`[0, i64::MAX]`）、`if` の式（両枝の hull）、不変の `let`、`for` 範囲ループのカウンター（昇順は `[start の下限, finish の上限]`、降順と `downto` はその逆、符号が不明なステップは両端の hull、`downto` でない符号なしは常に昇順）、`if` の then 枝の内側で比較が絞り込んだ不変のローカルから得ます。比較は同じ連言の中で結果が変わらなくなるまで（最大 3 回）適用し、`else`・`||`・`assert`・`match` からは絞り込みません。
それ以外はすべて型の全域です。`mut` のローカル、代入または可変借用されるローカル、複数回宣言された id、引数（条件がないとき）、呼び出し、`Index`、`@checked`、128-bit は旗を得ません。深さ 32 と、1 関数あたり 2^20 ステップを超えた式は、診断を出さず型の全域になります（GUIDE D-47: 解析の限界は旗を減らすだけで、プログラムの意味を変えません）。F12 の 65,536 ノードの上限を超える関数も旗を得ません。PM01 の共用体のタグ幅、PR03 の `Maybe`／`Result`／クロージャの lowering は、解析が各関数の最終の型付き IR だけを読み、配置を見ないので、旗の正しさには影響しません（型付き IR が変われば、付く旗の数は変わりえます。統合の後に audit と fuzz を再実行します）。
旗は `FunctionEmitter::binary` と、self tail call の引数の遅延（native だけ。WASM は直接評価）で書かれます。`Add.add`・`Sub.sub`・`Mul.mul` の `$intrinsic` の呼び出しは `binary` へ inline されるので、演算子形と組み込み形は同じ旗を得ます。旗のある関数は、PR01 が純粋な scalar 関数の戻り値へ付ける `noundef` を失います（`defined_result` が旗を poison の語として数えるため）。
検証は `cargo test --locked --lib interval`（区間関数を、ビットパターンだけを使う参照と、1〜4 bit の型のすべての区間と値の組、8〜64 bit の端、ネイティブの `overflowing_*` と照合）、`--lib ranges`（型付き IR を emitter と同じ順に歩いた 48 本の旗の表）、`--test integer_ranges`（IR の形・旗・audit の検査数・モード間の差が旗だけであること・38 本のプログラムを全入力で評価した結果との照合）、`node tests/integer_ranges_fuzz.mjs`（乱数のプログラムを BigInt の独立した評価器と、audit／proven／off の wasm32 で照合）、`node tests/range_flags_audit.mjs`（既存の suite を `audit` と `proven` で実行）で行います。`TSUZURI_RANGE_FLAGS=audit cargo test --locked` は、IR を出す Rust のテスト全体を audit で実行します。PM01 や PR03 の統合の後は、audit のテストと fuzz をもう一度実行します。
旗が LLVM の出力を変えるかは `benchmarks/run-range-flags.mjs` が `off` と `proven` で asm を比べます（結果は [整数の範囲証明と overflow フラグ（PR02）](benchmarks.md#整数の範囲証明と-overflow-フラグpr02)）。

### 整数の折り返しの報告（A23）

`--overflow-report`（`tsuzuri test` は既定）は、`src/overflow.rs` の `Scope`（スレッドローカル。`PlatformScope` と同じく `EmitOptions` を変えない）で driver から emission へ渡ります。`Scope` はソースの表（`Span::source` の添字ごとのパス・本文・報告するか。`Project::overflow_sources` が、ルートパッケージの利用者のソースだけを報告すると決める）と、emission が番号を振った地点の表を運びます。`emit_program_in` は scope があれば `Globals.overflow` に `llvm_overflow.rs` の `Report` を作り、`FunctionEmitter::new` が関数ごとに `Report::covers`（`ModuleOrigin::User`、ベンチでない、`$` で始まるモジュールは `$lambda`・`$task` だけ、利用者が書いたか持ち上げた関数、ファイルのトップレベルのコード（検査器が作る `$entry`）とテストの本体（同じく `$test.N`。`origin.test` で見分ける）、span のソースが報告するソース）を 1 回決めます（`overflow_counted`）。報告する関数では、区間の解析（`ranges::analyze` の `intervals`）も有効になります。
計装は 4 か所で、どれも元の演算の**前**に書き、演算の値には触れません: `binary` の整数の `+ - *`（`Add.add` などの PR02 D10 の inline もここを通る）、native の `tail_arguments` の遅延する `add`・`sub`（地点は引数の span）、`emit_expression_mode` の符号付きの単項 `-`、`llvm_exception.rs` の `power` の `checked == false` の積（地点は `**` の式 1 つ）。`overflow_check` は、その型の意味で証明できた演算（`ranges_proven`・`RangeFacts::negation_flags`。符号付きは `nsw`、符号なしは `nuw`）を飛ばし、ほかは `checked_integer_arithmetic` の旗を `llvm.expect.i1(.., false)` で冷たい枝にして、地点の番号 `i` の 1 byte を立てます。native は `atomicrmw xchg` で 0 から 1 になったときだけ `@tz.overflow.report` が定数のメッセージを `write(2)` で書き（`@tz.overflow.reported` の数が 100 に達したら打ち切りの 1 行）、WebAssembly は `store atomic ... monotonic`（`atomics` のない wasm32 で libcall にならない）だけです。地点は `(source, start, end, anchor, operation, bits, signed)` の `BTreeMap` で（`anchor` はメッセージが指す演算子の位置。`a + b` と `a + b + c` のように始まりが同じ地点を区別する。演算子のない `Add.add a b` は式の始まり）、emission が初めて計装した順に番号を振るので、特殊化の複本は同じ番号を共有し、IR と表は決定的です。`Drain::DepthFirst` の 2 回目の emission は表を作り直し、最後の emission が `Report::publish` で scope に表を渡します。`Report::runtime` は emission の後に `@tsuzuri_overflow_hits = internal global [N x i8]` と、native のメッセージと報告関数（`declare i64 @write(i32, ptr, i64)` は `llvm_traps.rs` と同じ綴りで、Windows の shim がそれを置き換える）、または WebAssembly の `tsuzuri_overflow_count`・`tsuzuri_overflow_hit` を書きます。地点が 65,536 を超えると `E1017`（`too_many_overflow_sites`）です。scope がなければ何も変わらず、旗のないビルドの IR は計装の前と byte 単位で同じです。
driver の `build_complete` は scope を入れて emission の後に地点を受け取り、WebAssembly では `overflow::side_table` を `<出力>.overflow.json`（キャッシュの `overflow`）に書いて `wasm-ld` に 2 つの export を足します。native の末尾呼び出しの引数の遅延は、報告があるときは利用者の補助関数（`def plus a b = a + b`）の呼び出しを展開せず、その関数の本体が自分の地点で報告します。`run_process` は `--json` で成功したとき、`overflow::json_lines` が報告を地点のメッセージ（行の位置は `overflow::LineIndex` で 1 回だけ数える）との完全一致で `W2001`（kind `integer_overflow_wrapped`、`path`・`span` 付き）に変えます。報告は行の中の最後の `REPORT_PREFIX` から（または行末の止まりの行。`overflow::report_bytes`）で、プログラムが改行で終えなかった出力が前にあれば、その byte を 1 行にしてから警告を書きます。ほかの行は、UTF-8 でなくても byte のまま渡します。`tsuzuri test` は `TestOptions.overflow`（`OverflowTests { fail, sources }`。`TestOptions::default()` は `None`）で `build_runner` に scope を入れ、`read_tail` が標準エラーを末尾 64 KiB とは別に走査して報告を最大 101 個集め（各行の最後の 4 KiB で `report_bytes` を探すので、改行で終わらない長い出力の後ろの報告も拾う）、`execute_test` が成功した終了のテストを最初の報告で失敗にします（`--allow-overflow` は `fail: false`）。WebAssembly のランナーは地点の表を `test-runner.mjs` の引数に渡し、実行の後に `tsuzuri_overflow_hit` を読んで native と同じ行を書きます。トラップ（`WebAssembly.RuntimeError`）とエンジンのスタックの上限（`RangeError`・`InternalError`。`trap-boundary.mjs` と同じ読み方）はテストの失敗で、その後も報告を書きます。
検証は `tests/integer_wrapping.rs`（`Int.wrapping_*`）、`tests/overflow_report.rs`（地点・intrinsic・scope・決定性・`run --json`・`tsuzuri test` の native と WASM）、`src/overflow.rs`・`src/llvm_overflow.rs` の単体テスト、`node tests/overflow_report.mjs`（付録 E の fixture を native と wasm32 の `-O0`・`-O3` で BigInt と照合し、標準エラーと `tsuzuri_overflow_hit` を確かめる。`TSUZURI_RANGE_FLAGS=audit` でも同じ）です。

### コンパクト文字列（PM03）

`string` は `{ ptr, i64 }` のまま、長さの語の bit 62 が立っていれば Latin-1（低位 62 bit が単位数で、1 単位 1 byte）、立っていなければ UTF-16（語が単位数）です。bit 63 は常に 0 です。PM01 が `Maybe<string>` の「なし」を長さ −1（`{ ptr null, i64 -1 }`、タグの判定は `icmp slt i64 length, 0`）に置いたので、印を bit 63 に置くと印のついた語が負になり、niche と区別できなくなります。長さの上限は 2^53 − 1 のままで、62 bit の数に必ず収まります。空の文字列は語が 0 で、どちらの形式でもありません。形式に正準形はなく、全単位が 0xFF 以下の UTF-16 の文字列（UTF-16 の文字列の `slice` など）も正しい値で、`==`・順序・`hash` は形式によらず同じ結果を返します。
`TSUZURI_STRINGS`（`src/llvm_strings.rs` の `StringForm`）が形式を選びます。`utf16`（既定）は Latin-1 を作らず、PM03 より前と byte 単位で同じ IR です（`FunctionEmitter::string_count` が語をそのまま返します）。`compact` は、語から形式を調べて両方の形式を読み、形式を実行前に決められる場所（リテラル、数・`char` の表示、Latin-1 同士の連結）で Latin-1 を作ります。変数は `cache::KEY_ENVIRONMENT` に入り、空は既定、ほかの値は `emit` のとき `E2000` です（`check` と `lsp` は IR を出さないので読みません）。ライブラリの入口は `llvm::emit_with_strings` で、環境変数を読みません。
生成コードは `%tz.string` の第 1 フィールドを、印を外さずに数として読みません。長さは `string_length`（`and i64 word, MASK`）、形式の判定は `string_is_latin1`、添字の読み出しは `string_unit`（Latin-1 は byte を `i16` へ広げる）／`string_unit_pointer` を通ります。`for unit in text` の変数は、Latin-1 では反復ごとに広げた 1 つのスロットの値で、文字列の中の場所ではありません。所有権検査（`ownership_control.rs` の `ForEach`）は `string` の反復変数を文字列の要素の別名にせず、`ref unit` を反復の外へ持ち出すと `E1013` にします（`utf16` にも同じ規則を適用し、2 つのモードで受け入れるプログラムは同じです）。`tests/compact_strings.rs` の lint が、生成した IR にある語の使い方を数え、印を外さずに数として使っている箇所を見つけます。リテラルは、全単位が 0xFF 以下なら byte の定数と印のついた語、そうでなければ従来どおりです。`string` の補間（`compact_interpolation`）は、すべての穴が Latin-1 か空で、断片と fill が 0xFF 以下のときだけ Latin-1 を作り、確保は 1 回です。`String.slice`・`String.join` は標準ライブラリの本体を差し替え（`compact_body`）、`@tz.string.slice`・`@tz.string.join` が結果の形式で 1 回の確保で書きます。`String.to_code_units` は Latin-1 を広げ、`String.from_code_units` は全単位が 0xFF 以下なら `@tz.string.from_units` でその場で狭めて縮めます（どちらも O(n)）。
runtime は加法の層です。`string.ll`・`display.ll`・`character.ll`・`format.ll`・`integer.ll` の定義のうち層が取り直すものは、`define` 行だけを `@名前.utf16` へ改名して残し（`layered`）、層が元の名前を定義し直して語の bit 62 を見て Latin-1 の kernel か `.utf16` の元の関数へ振り分けます。取り直す名前はコードの定数（`STRING_TAKEN` など）で、`src/llvm_strings.rs` のテストが、各名前の定義がちょうど 1 つあること、改名が `define` 行だけであることを確かめます。形式が混ざった連結は Latin-1 の側を広げて UTF-16 を作り（`concat_into` の 256 byte のスタックバッファには、UTF-16 なら 128 単位、Latin-1 なら 256 単位まで入ります）、`equal`／`compare` は両方が Latin-1 なら byte の kernel、混ざれば単位ごと（`equal_mixed`／`compare_mixed`）に比べます。`Bool`／`Unit` の表示の定数と書式指定の定数は UTF-16 のままです。
WebAssembly の heap は 1 つの block に上限があるので（モジュールの一部のとき。`--allocator host` を除く）、Latin-1 の確保（`allocate_latin1` と Latin-1 の `append`）は先に `@tz.string.latin1_fits` で、heap の上限に長さの 2 倍（UTF-16 の大きさ）を当てます。
超えるときは heap 自身の `@tz.alloc` を 2 倍の大きさで呼び、`utf16` と同じ確保の失敗の trap にします（上限の行は `with_wasm_heap_limit` が heap の 3 行と一緒に書き換えます）。
UTF-16 で作れない長さの文字列が `compact` にだけ存在することはなく、`to_utf16`・`borrow_utf16` が上限で断られることもありません。native には上限がないので、中身のない関数です。
Latin-1 の kernel の scalar 参照は `string_latin1.ll` で、`generate_utf.py` が同じ関数の 128-bit vector 版 `string_latin1_v128.ll` を書きます。選び方は PR05 の vector runtime と同じ（native、または `--wasm-feature simd128` の WebAssembly が vector 版。`string_runtime(utf16, vector)`）で、能力の検査が要る CPU 経路は使いません。`from_utf8` は `utf8_scan` で入力を 1 度走査し、ASCII と U+0080〜U+00FF の scalar だけなら `{ 単位数, 最大の scalar }`、それ以外（不正な列と U+0100 以上を含む）は `{ -1, 0 }` を返します。前者だけ `utf8_to_latin1` が Latin-1 を書き、後者は `from_utf8.utf16` の元の経路へ渡すので、不正な入力の trap は変わりません。vector 版の走査は 16 byte を ASCII・継続・先頭に分類し、先頭の数と継続の数が合う窓だけを受け入れ、末尾は scalar です。
host との境界は UTF-16 のままです。`export` の `string` の結果は `@tz.string.to_utf16`（Latin-1 なら新しいバッファへ広げ、UTF-16 はそのまま）を通してから返し（`llvm_abi.rs`）、host から受けた `ref string` と `main` の引数は UTF-16 の文字列として扱い、host の import へ渡す `string` は `@tz.string.borrow_utf16`（Latin-1 なら広げた複製）を渡して呼び出しのあとに解放します（`llvm_imports.rs`。複製は、すべての引数を評価し終えてから呼び出しの直前に作ります。コールバックを取る import は呼び出しの場所に展開されるので、あとの引数が `try` の中で例外を投げても、複製が残らないようにするためです）。ABI・host の binding・`--emit header` は `utf16` と同じです。デバッグ情報の `string` の型は変わらず、LLDB の formatter（`scripts/lldb/tsuzuri_lldb.py`）と natvis（`src/runtime/tsuzuri.natvis`）が語の bit 62 を見て、どちらの形式も表示します。
検証は `cargo test --locked --test compact_strings`（形式の名前・環境変数・IR の形・lint・niche・host の境界・反復変数の借用が反復で終わること（E1013）・コールバックを取る host の呼び出しが複製を最後の引数のあとに作ること）、`--lib llvm_strings`（層の合成）、`node tests/compact_strings.mjs`（`tests/fixtures/compact_strings` を `utf16`／`compact` × native／wasm32 × `-O0`／`-O3`、と simd128 の wasm32 でビルドし、すべての出力が同じで、どの呼び出しも確保を残さないことを確かめる。`tests/fixtures/compact_strings_import` では、コールバックと `ref string` を取る host の import（native は C、wasm32 は JavaScript の host）が UTF-16 のコード単位を読み、あとの引数が例外を投げても複製が残らないことを確かめる）、`node tests/compact_strings_raw.mjs`（生の長さの語を直接渡して runtime の関数を JavaScript の参照と照合し、確保した領域がすべて解放されることも確かめる。wasm32 の scalar と simd128 × `-O0`／`-O3`）、`node tests/utf_simd.mjs`（Latin-1 の kernel を scalar と照合し、過読みと再生成の一致も検査）、`python3 tests/lldb_strings.py`、`node tests/compact_strings_modes.mjs`（既存の文字列に関わる suite を両方の形式で実行）、`TSUZURI_STRINGS=compact cargo test --locked`（IR を出す Rust のテスト全体）で行います。`utf16` の IR が PM03 より前と同じことは、examples とテストの fixture をベースラインのコンパイラと byte 単位で比べて確かめました。
既定は `utf16` のままです。`compact` は確保量を ASCII・Latin-1 で約半分（`apps` の `tokenize` −24%、cpp の五つ −30〜−50%）、`ascii_words`・`latin1_csv` の RSS を約 3 割減らしますが、runtime の IR が 31〜49%、hello の wasm32 が約 17%（統合後の再測定で 6,860 → 8,058 bytes）、clang の時間が約 23% 増え、host へ短い文字列を返す呼び出しが 2 倍近くになる場合があります（M4 の基準を満たさない）。数値と方法は [PM03 コンパクト文字列](benchmarks.md#pm03-コンパクト文字列) にあります。既定を替えるには、使わない kernel と振り分けの除去（`RuntimeSymbols`）、host へ返す結果の UTF-16 での直接生成、Linux・Windows の allocator での再測定が要ります。

### 比較の借用

`Eq` および `Ord` のメソッド型は共有借用（`ref`）を受け取るシグネチャを持ち、比較演算子における所有権検査はすべての型において `Use::Read`（読み取り）として処理されます。
組み込みスカラー型以外の比較演算子は `Call(method, BorrowOperand(left), BorrowOperand(right))` へと単相化され、被演算子がメモリ上の場所であればポインタを、一時値であれば `frame_value` によるスタック値をエントリーブロックのスロットへ保存して渡します。
一時値は関数呼び出しの完了後に、左から右への順序で `drop_framed` により安全に解放されます。ソースコード上の `Borrow` 制約の意味論が変害されることはありません。
数値や文字列の組み込み演算子は直接の lowering 経路を維持し、メソッド値ラッパーのみが借用を参照外しして同一の組み込み命令を実行します。

### 型別名

`Names.type_aliases` は宣言元モジュール、可視性、および右辺の `TypeExpr` を保持し、レコード型や union と同一の名前解決規則を使用します。
`TypeAliasExpansion` は右辺の型式および型引数を各モジュールのコンテキストで正しく解決したうえで型式を展開します。型クラス制約の適用も型式として保持され、`inline_constraints` および格納型の制約検査へと引き継がれます。
循環参照は `E1024` エラー、ネスト深度 128 または走査要素数 4096 の超過は `E1017` として拒否され、展開結果に対して型サイズの有界性検査（`bounded_type`）が適用されます。
未使用の型別名宣言であっても厳密に型検査が行われ、公開（public）別名の右辺に非公開（private）型が出現していないかを検査する漏洩チェックも漏れなく実施されます。
型付き IR や LLVM コード生成のフェーズには型別名専用の表現は残存せず、インスタンス解決や関数の特殊化ではすべて展開後の具象型が使用されます。

### 型パラメーターの構文

レコード、union、型別名、型クラスの宣言、型適用、制約、インスタンス、および `Task<T>` の記述は、すべて共通の山括弧 `<...>` とカンマ区切りの構文で解析されます。型名と `<` は空白を挟まずに隣接させ、型引数の内部では完全な `type_expr` を解析するため、入れ子の型適用、関数型、借用型を記述する際に追加の丸括弧で囲む必要はありません。
空の型引数リスト `<>` は拒否されますが、末尾のカンマは許容されます。型パラメーターの個数およびネスト深度には既存の最大 128 の上限が適用されます。
型の閉じ括弧 `>` をパースする際は、トークンストリーム中の `>>`、`>>>`、`>=` から 1 文字ずつ `>` を消費することで、合成トークンを挿入することなく正確なソース位置（Span）を保持します。式の比較演算子やビットシフト演算子は従来のトークンとして解析されます。
旧来の空白区切りの型引数構文は受理されません。暗黙の全称量化、型推論、型付き IR、および LLVM 上の正規型名には影響しません。

### ジェネリックなレコード

構文 AST 上の名前付き型適用は `TypeExprKind::Apply(head, args)` に一元化されており、`Add<'a>` のような型クラス制約と `Pair<i64, string>` のような型の具体化は、型解決のフェーズで識別子の種類に基づいて正確に分類されます。
クラス名とレコード名は同一の名前空間を共有するため、未知の型名、型引数の個数不一致、または非ジェネリック型に対する型引数の適用は `E1004` エラーとなります。
型付き IR におけるレコード型は `Type::Record(id, args)` として表現され、非ジェネリックなレコードは空の引数列を保持します。
型引数列は `Box<[Type]>` として内部でヒープ確保して保持することで、`Type` 列挙型全体のサイズを 32 バイトに抑え、2 MiB スタックのテストスレッド環境でも深い式の再帰検査でスタックオーバーフローを起こさないように設計されています。`TypeExprKind::Apply` も同様の理由からヘッド名と引数列をボックス化して保持します。
レコード宣言のフィールド型には型パラメーターを含めることができ、`TypeContext` の `record_fields` および `record_field` が実引数で置換された具象フィールド型を返却します。Copy、drop、clone、loan、Capture、Send の各特性判定、所有権の部分 move、呼び出し特殊化の可変性判定、およびスタックフレーム配置の計算は、宣言時の抽象型ではなく必ず置換後の具象型に基づいて行われます。
メモリレイアウト計算（最大サイズ 64 KiB 上限および再帰型の検出 `E1010`）や、参照型の格納禁止チェックは具象型ごとに行われ、シグネチャ、型注釈、リテラル、および単相化後の式型の各境界で厳格に再検査されます。
LLVM コード生成では、非ジェネリックなレコードを従来どおり `%tz.record.M.N` として宣言順に出力し（定義の並びの話で、各定義の中のフィールドは下の格納順です）、続いて出力対象となる全関数のシグネチャ、ローカル変数、式型、および非ジェネリックレコードのフィールドから具象インスタンスを `BTreeSet<Type>` に収集し、入れ子のインスタンスまで推移閉包を求めたうえで 1 回ずつ決定的に定義します。
型名は `%"tz.record.M.Pair[i64,string]"` のような正規の型文字列（`array[T]`、`list[T]`、`tuple[T,U]`、`fn[T,U->R]`、`ref[T]`、`refmut[T]`、`task[T]` など）となり、ジェネリック宣言そのものの抽象型は LLVM IR に出力されません。
集合の順序と正規名はソースの記述順やハッシュ値に左右されず、ネイティブおよび WASM の双方で完全に決定論的な IR を生成します。

### レコード更新

`{ base with field = value; ... }` の構文は、最初の式を通常のパーサーで読み取った後に続く `with` キーワードの存在によって識別されます。
`match` 式の内部に出現する `with` との混同を防ぐため、単純なトークン先読みによる判定は行いません。AST 上では `RecordUpdate` としてベース値と更新フィールドのリストを保持します。
型付き IR でも記述順序を保持する専用の `RecordUpdate` ノードへと lowering され、子要素の走査順序および所有権検査も記述順に厳密に従います。
LLVM コード生成では、元となるレコードとすべての更新値式を先行して評価し、置換対象となる古いフィールドの値を適切に解放（drop）した後に、LLVM の `insertvalue` 命令によってインプレースに値を差し替えます。
更新されない残りのフィールドに対しては通常のフィールド射影による余分なコピー（Copy）は行わず、元の集約値をそのまま引き継ぎます。

### 共用体（union）

構文 AST の `UnionDecl` は、各 case ごとに 0 個または 1 個のペイロード型を保持し、`of` は case 宣言の内部でのみ有効な文脈キーワードです。union の型名は型クラスやレコードと同一の型名前空間に属し、case 名は同一モジュール内の型名、関数名、アクティブパターン名と衝突しない値の名前空間に属します。名前の衝突は宣言順序にかかわらず `E1001` エラーとなります。
型付き IR における型は `Type::Union(id, args)`、宣言情報は `CheckedModule.unions` の `CheckedUnion` で管理され、`TypeContext::union_payload(s)` が実引数で置換された具象ペイロード型を返します。Copy、drop、loan、Capture、Send などの特性は、レコードと同様に置換後の全ペイロードから構造的に判定されます。
関連する IR 式ノードとして、値の構築を行う `Construct { union_id, case_id, payload }`、case をファーストクラスの関数値として扱う `CaseConstructor`、case 番号を 32-bit 整数の値として読む `UnionTag`（格納するタグの幅は case の数で決まり、読むときに `i32` へ広げます）、および case のペイロードを取り出す `UnionPayload` が用意されています。
全引数が完全に適用された case コンストラクタ呼び出しは、`Checker::call_kind` によって即座に `Construct` へと変換され、不要な関数呼び出しのオーバーヘッドを残しません。
完全適用されず関数値として残存した `CaseConstructor` は、再帰検査および単相化の完了後に `closures::lower` が `(union ID, case ID, 具象型引数列)` ごとに 1 つの非公開関数 `$case.M.U.C[.$mono.N]` へと置き換えるため、再帰呼び出しの診断メッセージに内部合成関数が漏洩することはありません。
case パターンマッチは、タグの等価判定 `Test(UnionTag(subject) == case)` とペイロード射影 `UnionPayload` へと分解されます。所有権検査において `UnionPayload` は、コレクションの要素（`ELEMENT`）と区別された独立したメモリ場所成分 `PAYLOAD` として追跡されます。
ペイロードの move は union の部分 move として扱われ、移動元のペイロード領域は即座にゼロクリアされるため、後続のスコープ終了時に union 全体を解放する際にも、移動済みのペイロードが二重解放される危険はありません。

メモリレイアウトの保守的な見積もり（`Layouts::union` および `llvm_frame::stack_size` で共有）は、ペイロードなしの場合は 8 バイト、ペイロードありの場合はタグ用の 16 バイトと最大ペイロードサイズを 16 バイトアライメントに切り上げた値の合計となり、64 KiB のサイズ上限および再帰型の検出（`E1010`）が具象型ごとに実施されます。再帰的な union は後述のヒープ所有ノード表現へと移行します。
LLVM 上でのメモリレイアウトは `llvm_layout.rs` の `UnionLayout` によって決定されます。タグは case の数に応じた幅で保持します（256 個までは `i8`、65,536 個までは `i16`、それを超えると `i32`）。読む側は格納した幅でロードし、`zext` で `i32` に広げてから比較・`switch` します（符号拡張は使いません）。すべての case にペイロードが存在しない場合は単なるタグの整数（`Enum`、たとえば `i8`）、すべての case のペイロードが同一の LLVM 型である場合は `{ iT, T }`（`Common`）、それ以外の不均一なケースでは `{ iT, [N x iU] }`（`General`）となります。
`General` の領域は、各ペイロードのアライメント（`storage_layout` の上限）のうち最大の値を 16 で頭打ちにした `U` バイトの整数が `N` 個並んだ配列です。`N` は最大ペイロードの大きさ（同じ上限）を `U` で切り上げた個数で、領域はタグの直後を `U` に切り上げた位置から始まります。`Circle of f64 | Rect of f64 * f64 | Empty` は `{ i8, [2 x i64] }`（24 バイト）、`Flag of bool | Code of i8 | Nothing` は `{ i8, [1 x i8] }`（2 バイト）、`i128` を持つ case があれば `{ i8, [1 x i128] }`（32 バイト）です。256-bit ベクトルを持つペイロードの領域も 16 バイト境界までで、そのロードとストアは `with_vector_alignment` により `align 16` で行います。ペイロードのない Drop union は `{ iT, [0 x i8], i8 }`（`General { unit: 1, units: 0 }`）です。ペイロードの読み書きは領域への GEP とペイロード型による `load`／`store` 命令によって行われます。再帰 union のノード `{ ptr, ptr, ptr, i32, 領域 }` は `i32` のタグを保ち、領域は上の `Common` のペイロード型か `General` の配列です。
case がちょうど 2 つで、1 つがペイロードなし、もう 1 つのペイロードが `string`・`utf8string`・配列（`{ ptr, i64 }`）、関数値・`Task`（`{ ptr, ptr }`）、または、ネイティブのターゲットだけ、スライスでない `ref`（`ptr`）の union は、タグを持たずペイロードそのものです（`Niche`、`Maybe<string>`・`Maybe<ref T>` など）。文字列と配列では長さが負の値（`{ ptr null, i64 -1 }`、`NegativeLength`）、`ref` では null ポインタ（`NullPointer`）、関数値・`Task` では欄 0 の記述子が null の値（`NullDescriptor`。`None` は `zeroinitializer`）がペイロードなしの case で、どの文字列・配列の長さも負にならず、どの `ref` も null にならず、どの関数値も静的記述子を指すのでペイロードと区別できます（記述子は静的データで 0 番地に置かれないので、`NullDescriptor` は WebAssembly でも使います。環境が null の関数値は記述子を持つので `Some` です）。`TSUZURI_STRINGS=compact` の Latin-1 の印は長さの語の bit 62 で、bit 63 は常に 0 なので、印のついた語も負になりません。WebAssembly は `ref` に `NullPointer` を使いません。`--stack-first` ではアドレス 0 も有効なスタック上のオブジェクトの位置になり得るためで、`Maybe<ref T>` は `{ i8, ptr }` のままです（`NegativeLength` はターゲットによらず適用されます）。レイアウトはターゲットごとに決まり（同じターゲットでは常に同じ）、`llvm_type` と `union_layout` は `emit_program` が張るスレッドローカルの `target_scope` から WebAssembly かどうかを読みます。`llvm_type` がペイロードの型を返すため独自の型定義はなく（`Maybe<string>` は `%tz.string` の 16 バイト、ネイティブの `Maybe<ref T>` は `ptr` の 8 バイト）、ペイロードの場所は union の場所そのものです。タグは、長さを読んで `icmp slt i64 .., 0`、ポインタか関数値の欄 0 を `icmp eq ptr .., null` と比べて `select` で case 番号（`i32`）にし、構築はペイロードの値（`Some`）か上の定数（`None`）です。`match` の `switch` と所有権・解放の規則は他の union と同じで、解放はタグが `Some` のときだけペイロードを解放します（`None` の長さ -1 で配列の要素を巡回しません）。再帰 union、Drop union（生存フラグを持つ）、case が 3 つ以上または両方にペイロードがある union は対象外です。move 元を `zeroinitializer` で消した値は長さ 0 の `Some ""` と読まれ、その解放は `free(null)` の no-op で安全ですが、ゼロ値が `None` を表すとは限らないので、`Seq.next` は空の `Seq` と `None` を `construct_value` で明示的に作ります。戻り値スロット（`llvm_return_slot.rs` の `uses_return_slot`）など、tag と payload を持つ aggregate だけを相手にする処理は `UnionLayout::is_aggregate()`（`Common` と `General`）で尋ねます。DWARF は `$payload`（ペイロードの case をメンバーとする C の union）に、ニッチの位置の列挙型 `<型名>.$niche` を重ねた構造体です（`NegativeLength` は長さの位置（オフセット 8）の `i64` で `None = -1`、`NullPointer` はポインタの位置（オフセット 0）の符号なし 64 ビットで `None = 0`、`NullDescriptor` は記述子の位置（オフセット 0）の符号なしのポインタ幅（ネイティブ 64 ビット、wasm32 32 ビット）で `None = 0`）。
サイズの上限と下限は 3 つのモデルが共有します。`layout_bound(.., false)`（`storage_layout`）は割り当てと `llvm_frame` の環境予算に使う上限で、`i128` を 16 バイトに揃える ISA（AArch64、LLVM 18 以降の x86-64、wasm32）の実サイズを下回りません。`layout_bound(.., true)` は `ref` 引数の `align N dereferenceable(S)` に使う下限で、`i128` を 8 バイトに揃える旧 LLVM の x86-64 の実サイズを上回りません。`check.rs` の `Layouts` は `E1010` のための保守的な見積もりで、実際のレイアウトからは独立に保ちます。単体テスト `layout_bounds_cover_actual_sizes` は、出力した型定義から 5 つの data layout（AArch64、x86-64 の `i128` 8/16 バイト、wasm32 の同 2 通り）の実サイズを別実装で求め、両方の境界が実サイズを挟むことを確かめます。
LLVM 型名は `%"tz.union.M.Shape"` や `%"tz.union.M.Maybe[i64]"` のようになり、ジェネリックレコードと同様に `named_instances` の推移閉包を求めて具象インスタンスのみが決定論的な順序で 1 回ずつ定義されます。
### record のフィールドの格納順（PM01 Phase 3a）

ユーザーのプログラムのレコード（`ModuleOrigin::User`。標準ライブラリのレコードは、ランタイムと手書きの IR が添字で読むので対象外）は、具体的な型ごとに、フィールドを `storage_layout` のアライメントの降順で安定ソートした順（同じアライメントは宣言順）に格納します。ソートしたモデルの大きさが宣言順より厳密に小さいときだけ採用し、そうでなければ宣言順のままです（`place_fields`）。`Mixed { bool, i64, i8, f64 }` は `{ i64, double, i1, i8 }`（32 → 24 バイト）、`Spread { i8, i64, i8, i32, bool }` は `{ i64, i32, i8, i8, i1 }`（32 → 16 バイト）、`Pair { i8, i64 }` は同じ 16 バイトなので宣言順です。Drop レコードの生存フラグは常に最後で、添字は `fields.len()` のままです。`record_slots(id, arguments, module)` が論理添字から LLVM 型の位置への表を返し（`None` は宣言順）、`field_slot` が唯一の読み手です。`Field` の `extractvalue`・`getelementptr`、リテラルと更新の `insertvalue`、部分 move の残りのゼロ化、drop と clone の接着、frame の `frame_aggregate`・`relocate`・`drop_framed`（`Frame::Aggregate` のキーは論理添字）、`static_constant`、型定義、ホストの record の変換が通ります。観測できる動作は論理順（宣言順）のままです。評価と drop の順、`deriving` の比較・Hash・Display（AST から作るので論理添字）、DWARF のメンバー（宣言順で、offset だけが格納位置）、公開 ABI の C の構造体（`record_layout`・`header_types` は宣言順で、`read_host_record`・`write_host_record` が内部側の添字だけを `field_slot` にする）がそうです。`layout_bound` は上限も下限も格納順で畳み、`check.rs` の `Layouts`（`E1010`）・`const_static` の予算・`llvm_frame::stack_size` は、宣言順の保守的な上限のままで実際の大きさを下回りません。`union_layout` と `record_slots` は、`emit_program` が張る `target_scope` の memo に覚えます（モジュールのアドレスで区別し、scope の終わりで捨てます。`CheckedModule` に `RefCell` を足さないので、`Sync` のまま保てます）。タプルは並べ替えません。`Mutex` のセルは先頭の語を runtime が lock として読み、添字を手書きするタプルの builtin が約 10 あるためで、将来 `Declared` の例外を作る場合の前提は ticket `PM01` に書いてあります。
メモリの解放および複製処理では、タグ値に対する `switch` 命令を用いて現在活性化している case のペイロードのみを処理し、非活性のペイロード領域を誤って読み出したり解放したりすることはありません。
なお、`Construct` による union の生成はスタックフレーム管理の対象外であり、union の値は `new` を付けないリテラルであってもフレーム領域を持たず直接構築されます。

### 再帰構造の解析

`recursive::analyze` は、具体的な record および union と、タプル、配列、リスト、Vec などの格納エッジを包括的に走査します。
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

### 利用者定義の解放（B07）

組み込み型クラス `Drop` のインスタンスヘッドとして宣言された型には、`CheckedRecord::user_drop` または `CheckedUnion::user_drop` のフラグがセットされます。
このフラグはインスタンス収集の直後、スーパークラスの検証および関数本体の型検査よりも前の段階で付与され、`Type::has_user_drop` を通じて `is_noncopy_record`（Copy 判定で最初に参照される）、`needs_drop`、および `can_capture` へと確実に伝播します。
単相化の完了後、特殊化された関数の型シグネチャが所有するすべての Drop 型（フィールド、ペイロード、コレクション要素、Task の結果。参照型や関数型は辿らない）を収集して `drop` メソッドを特殊化し、不動点解析の結果を `CheckedModule::user_drops`（`BTreeMap<Type, usize>`）に格納します。`Drop` インスタンスが存在しないプログラムではこの走査は行われず、生成される IR も変化しません。
所有権検査は `Place::through_drop` により、Drop 型の境界を通過するフィールドやペイロードのパスを記録し、その内部からの部分的な move、Drop 型に対する `RecordUpdate`、および `drop` メソッド本体における引数全体への再代入を厳格に禁止します。
呼び出し先での値の置換を防ぐため、`drop` メソッドの本体内では引数値全体の排他的な再借用や、`ref mut` 引数そのものの move も拒否されます（`Checker::drop_root`）。
LLVM は move 済みのメモリ領域を `zeroinitializer` でクリアし、その解放処理を安全な no-op（無処理）とするセマンティクスを徹底しています。そのため、非再帰の Drop 型はレコードのフィールド末尾（union ではタグとペイロードの直後）に `i8` の生存フラグ（liveness flag）を保持し、値の構築時（レコードリテラル、スタックフレームの集約、`construct_value`）に `1` に設定します。すべての case が引数なしの Drop union であっても、単純なタグ整数ではなく `General { unit: 1, units: 0 }` のレイアウト（`{ iT, [0 x i8], i8 }`）が選択されます。
デストラクタコード（`drop_value` および `drop_framed`）は、生存フラグが 0 でない場合にのみ値をエントリーブロックのスロットに配置してユーザー定義の `drop` メソッドを直接呼び出し、読み直したフィールド値を宣言順に解放します。
ゼロクリアされた領域では生存フラグが 0 となるため `drop` は呼び出されず、各フィールドの解放も従来どおり安全にスキップされます。一時的な Copy 型のフィールドやペイロードを読み出す際は、値を破壊することなく全体を安全に解放します。
再帰的な Drop union では先頭の引数なし case もノードとしてヒープ上に配置され（NULL ポインタは move 済みのみを表現します）、型ごとの drop ヘルパーが各ノードに対して `drop` を呼び出してから子ノードを待ち行列へ追加します。
`user_drops` に登録された関数群は本体コードから明示的には呼び出されないため、デッドコード削除で誤って除去されないよう到達可能性解析のルート集合に追加されます。なお、Drop を実装した record 型は C 言語 ABI のスカラーレコードとしては扱われません。

`use` 束縛は、`syntax::Binding::using` フラグを持つ特殊な `let` 束縛であり、型検査によって対象の値の型が `Drop` を実装していることを必須とします。コンピュテーション式内の `use!` は、生成された継続関数の引数を `use` で再束縛し、`task` ビルダーにおける `use!` は `let!` と同様の `TaskRun` として処理されます。
`Owned.drop`、`Owned.function`、および `Owned.call` はコンパイラ組み込み関数として提供され、`std/Owned.tz` では不透明な非 Copy 型である `Owned.Function<'a, 'b> { run: 'a -> 'b }` のみを宣言します（std 関数を無用に増やすと生成関数の採番がズレて既存の IR 決定性が崩れるのを防ぐためです）。
`Owned.function` の引数に直接記述されたラムダ式は `TypedExprKind::Lambda::owned` となり、自由変数の捕捉に対して通常の `Capture` 制約ではなく `Send` 制約を課します（単相化後の再検査でも同様です）。
`closures::lower` は持ち上げられた関数に対して `owned_captures` フラグを設定し、`TypedExpr::consuming_use` を通じて、非 Copy な捕捉値（デストラクタを持たない `extern type` のハンドルを含む）が関数本体によって不用意に move されないことを静的に検証します。
LLVM はその関数の捕捉引数を借用ローカル変数として生成し、apply アダプタは本体を呼び出した後、所有権を消費する呼び出しである場合にのみ `@tz.env.drop` を呼び出して環境を破棄します。
環境のクローン関数は生成されず（静的記述子の clone 欄は常に null）、`can_borrow` フラグは偽に設定されて、既知クロージャの直接呼び出しや借用ワーカーの特殊化最適化からは除外されます。
`Owned.call` は環境の借用フラグ（`i1 true`）を指定して apply を呼び出します。また、`Owned.Function` 自体は `can_capture` を満たさないため、誤って `clone_value` へ渡されることはありません。

`match` 式のコンパイルでは、`llvm_control::SwitchPlan` が最適化を担います。ガード条件を持たないすべての match 節の判定条件が単一のタグ値（整数、bool、unit の場合は値そのもの）の定数比較であり、かつ各節の束縛が同一の射影パスを持つ場合に限り、対象領域からタグを格納幅で 1 回だけロードし、`i32` に広げて `switch i32` 命令へと変換します。
各節の束縛値は `UnionPayload` または構造体フィールドの射影ポインタから直接読み出され、データ構造全体の不要な別名（alias）を作成しません。
OR の各側の束縛経路は一致させ、条件を満たさない match は従来の順序付き検査に戻します。

### カリー化

関数シグネチャの型宣言には `def`、実装には `fn` または宣言に対応する `let` とラムダ式を使用します。
型の矢印記号 `->` は右結合、関数適用は左結合です。内部の `Type::Function` が保持する引数列は連続する矢印列を正規化した表現であり、非カリー化関数を意味するものではありません。実装側の `Signature.parameters` は各ワーカー関数が実際に受け取る引数列を保持し、途中で関数を返却する関数の評価を後続引数の評価より後へ遅延させることはありません。
すべての引数が完全に適用された既知のワーカー関数は直接呼び出されます。一方、部分適用された呼び出しや未知の関数値に対する適用では、静的記述子を指す関数値（`%tz.closure = { ptr, ptr }`）と、1 引数ずつ処理するアダプタ関数が使用されます。
### 関数値の表現（PM02）

関数値と `Task` は `%tz.closure = { ptr, ptr }`、つまりポインター 2 つ（ネイティブは 16 バイト、wasm32 は 8 バイト）です。欄 0 は静的記述子 `@tz.desc.<修飾名>.<捕捉数>` を指し、欄 1 は環境（捕捉が無ければ null）です。記述子は `%tz.closure.desc = { ptr code, ptr clone, ptr drop }` の `internal unnamed_addr constant` で、`closure_wrappers` が `@tz.apply.*` ごとに、それだけで 1 ブロックとして出します（`closure_slots` が clone／drop を決めます）。`clone` は環境を複製する関数、`drop` は環境を解放する関数で、環境を所有しないものは null です。捕捉が無い関数値（環境が null）と、環境の欄に値そのものを入れる即値の捕捉（`immediate_capture`）は clone も drop も null、`Task` と B07 の `owned_captures` の関数は clone が null で drop だけを持ちます。したがって環境が null でなく drop が null の関数値は即値だけで、`@tz.closure.clone`・`@tz.closure.drop` は環境が null か記述子の drop が null なら何もせずに返します（`tests/closure_representation.rs` の `closures_built` が、4 種類の関数値の例と Task・`owned_captures` の例で、組み立てた全クロージャについて確かめます）。関数値の呼び出しは `FunctionEmitter::closure_parts` が、欄 0 の記述子から先頭の `code` を load し、欄 1 を環境として取り出します（`@tz.apply.*` の引数列は変わりません）。大きさの上下限は `layout_bound` で (16, 8)／(8, 4) で、PR01 の `dereferenceable` の下限もこれに従います。値のインラインレイアウトの上限（E1010）の見積もり（`check.rs` の `Layouts`）と `llvm_frame.rs` の `stack_size` は、関数値と `Task` を 32 バイトとして数える保守的な値のままです（実際の大きさ以上であることはテストで確かめ、見積もりを小さくすると受理するプログラムが変わるので変えません）。ネイティブでは、記述子が adapter を参照し続けて「ローカル関数の最後の呼び出し」の inline の加点が消えるので、`@tz.apply.*` と `$lambda` の worker（`-g -O0` を除く）に `alwaysinline` を付け、既知の関数値の呼び出しが畳み込まれるようにします（`inline_closures`）。ただし worker に付けるのは、adapter 以外の呼び出しが 1 か所以下のときだけです（`limit_worker_inlining`。捕捉のないラムダは既知なら名前で直接呼ばれるので、属性は本体を呼び出しの数だけ複製します。24 か所から呼ぶ 40 ステップのラムダは、`-O3` で 3,032 → 59,144 バイトになりました）。IR は最適化レベルによらず同じにはなりません。ドライバーが native の `-O0`・`-Os`・`-Oz` で `@tz.apply.*` と `$lambda` の worker から属性を外し（`llvm::without_closure_inlining`）、inline を LLVM に任せます。WebAssembly では付けません。残った adapter が worker の複製を持って、同じプログラムが 30〜50% 大きくなったためです。DWARF の関数値は `descriptor`（`closure.descriptor` の `code`・`clone`・`drop` へのポインター）と `environment` の 2 メンバーで、`scripts/lldb/tsuzuri_lldb.py` の `is_function` はその形で関数値を見分けます。

通常のアダプタ関数の内部引数順序は「値引数、環境ポインタ、`i1 borrow`」となっており、戻り値レジスタを次回の呼び出しの値引数レジスタとしてそのまま再利用できるように配慮し、ループ反復ごとの不要なレジスタ退避・移動を排除しています（なお、引数を持たない thunk や 1 回実行 Task の呼び出し規約は変更しません）。
借用が残存しないことを静的に証明できる途中段階では一時的な借用（loan）を終了させ、判定が困難な場合は安全側に倒して保守的に保持します。

### 捕捉（キャプチャ）

ラムダ式内の自由変数は一意な識別子（ID）によって検出され、ラムダ式自身は捕捉引数を受け取るワーカー関数へと持ち上げ（lambda lifting）られます。
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

### ローカルな `let rec`（B09）

`Binding.recursion`（`None`／`Rec`／`And`）が `let rec` と `and` のメンバーを表します。型検査はグループの名前を先に束縛してから各ラムダを検査し、`TypedExprKind::Lambda.recursive` にグループのメンバー（束縛順）を記録します。空なら通常のラムダです。
リフトは `src/closures_rec.rs` が行います。グループの捕捉は、各メンバーの捕捉と、本体が参照する前の `let rec` 関数・外側の `let rec` 関数のうちグループが直接呼べるものの捕捉の和（ローカル ID の順）で、全メンバーが同じ捕捉引数を受け取ります。
捕捉は、グループを束縛した本体のローカルと、グループのメンバーの引数としてしか存在しません。そこで下ろしている本体を `Frame`（リフトしたメンバーの本体と、通常のラムダ式の本体ごとに 1 つ。`lower_body`）で表し、`Frame.direct` に、その本体が直接呼ぶ関数（グループ自身と、グループが捕捉を引き継いだ前の関数・外側の関数）を持たせます。グループが前の関数の捕捉を引き継げるのは、その関数が `direct` にあるか、同じフレームで束縛されていて、値で渡す捕捉が関数全体のどこでも代入・可変借用されていない（`mutated`）ときだけです。閉包は作った時点の値を持つので、代入があれば今の値を渡す直接呼び出しは結果を変えます。引き継げない関数は、通常のローカル値として（関数値は常に借用で）捕捉し、閉包を通して呼びます。ラムダ式や継続の本体、入れ子のグループを定義した本体は、自分のフレームの `direct` に入っていない `let rec` 関数を、どれも関数値のままにします。
複製が値のコピーで済み解放処理を持たない捕捉（`is_copy` かつ `!needs_drop`）は値で渡し、それ以外は共有借用にします（リフトした関数の引数は `ref T`、閉包の環境は `Borrow(Local)`）。この分類は特殊化の後の具象型で行うので、型変数を含む型の捕捉は `polymorph` が `E1005` で拒否します（関数型の捕捉は、型変数を持たない関数値なので対象外です）。
各メンバーのリフト先の ID は、どの本体を下ろすより前に予約します。そのメンバーのフレームの `direct` にある名前は、関数適用・`x |> f`・部分適用なら「捕捉 ++ 引数」を渡す `Function(User(id))` の直接呼び出しに、それ以外の使い方なら `Closure(id, 捕捉)` に、借用した捕捉の読み出しは `Dereference` に、明示スタックで書き換えます。書き換えの前に、借用した非 Copy の捕捉をムーブする使い方（`TypedExpr::moving_use`）を `E1012` として検査します。`moving_use` は `consuming_use` と同じ走査で、入れ子のクロージャが捕捉してムーブする場合のエラーをクロージャの位置にし、`match` の対象を、非 Copy の値を取り出す束縛があるときだけ消費として扱います。`consuming_use` は、呼び出しの特殊化（`call_specialization::can_borrow`）と所有する関数の検査も使うので、変えません（変えると、`let rec` を書かないプログラムの IR が変わります）。
`ownership::eval_lambda` も、`let rec` のラムダの捕捉のうち値で渡せないものを共有借用として評価し、リフトの後と同じ判定にします（グループの関数が 1 つの所有値を捕捉してもムーブにはなりません）。
`let rec` を書かないプログラムでは ID の予約も書き換えも起きないので、生成される IR は変わりません。

### 純粋性の検査

`src/pure_rules.rs` は generic なテンプレートを特殊化前に検査し、特殊化と `closures::lower` の後でも、
`FunctionOrigin.pure` の宣言と純粋な関数値の根を検査します。明示した純粋関数値の本体と閉じた callback の捕捉も、
未使用の generic template が消費される前に同じ走査で検査します。通常の factory 自体の効果と、通常の戻り callback の実行には保証を広げません。
関数値の証明根は実関数のキャッシュと分け、局所再帰で同じ本体が再び現れたときは走査済みとして扱います。
生成した補助関数を無条件の純粋な根にはしません。
引数の `own` と共有借用の補完は署名の収集時に終わり、以降は既存の参照と所有権規則を使います。
純粋な矢印は `FunctionParameters.pure` に呼び出し段階ごとの契約を持ち、引数の物理的な規約を変えません。
parser と class の instance／既定実装は同じ正規化を使い、純粋な宣言の段階を超えて戻り lambda を取り込まず、
段階の全引数を束縛しない実装を `E1029` にします。途中の計算を動かす eta 展開はしません。
モジュール関数制約の推論と特殊化は、既存の純粋フラグの弱化だけを行って通常の型単一化を使います。
対象自身の署名と factory の物理的な段階を再構築・平坦化しません。
通常の矢印へ弱める cast は closure の表現を保ち、一時 callback の借用は cast の外側へ正規化します。
同じ引数・結果の型と規約を持つ段階の cast は直接呼び出しの特殊化でも透過に扱いますが、arity が変わる factory の境界は透過にしません。
AST の純粋な矢印・class／instance の印と、括弧で区切る pure factory の解析を保存する frontend の形式は 9 です。
各関数の局所の効果と関数参照の辺だけをキャッシュし、根ごとに幅優先で到達可能性を求めます。
探索中の再帰関数を純粋としてキャッシュせず、循環を含む別の根にも効果が伝播します。
閉じた Task／Parallel は callback と捕捉値を検査し、標準 IO の構築は遅延 callback の実行と区別します。
class の契約は既定実装と全 instance、generic の特殊化、`@cpu` の版へ引き継ぎます。
式の走査は明示的なスタック、関数の探索はキューで、関数 4,096 個・式 2,097,152 個の上限を超えると `E1017` です。
型・組み込み・型付き式の分類は網羅的な match とし、新しい variant は分類を決めなければコンパイルできません。
D-53 の `what`・`impurity_of_roots`・`reachable_effect`・`builtin_impure` が問い合わせの境界です。
GPU の抽出も `reachable_effect` を使いますが、既存の scalar・演算・再帰・backend の制限は別にすべて検査します。

`src/llvm_facts.rs` は純粋性とは独立に実際の命令からメモリ効果を証明します。`reuse_pure_calls` は、
明示の純粋な宣言と出力済みの `memory(none)` の両方がある、値の整数引数・整数結果の関数だけを再利用します。
同じ基本ブロックで最初に実行した呼び出しを残し、同一の SSA 引数の後続呼び出しだけを置き換えます。
整数の spill は、関数全体でアドレスが逃げない単純なスロットだけを、ブロック内の store／load に従って正規化します。
borrow・atomic・volatile・未知のアクセスはこの正規化から除外します。前方参照する loop phi も含めて SSA 使用を置き換え、
quoted text・コメント・名前の前方一致は書き換えません。数値番号の SSA 定義と quoted な定義も再利用せず、番号や未対応の文法を壊しません。
関数が 200,000 byte を超える場合は最適化を省略します。参照・浮動小数点・所有結果・CPU multiversion は対象外です。
純粋性から新しい LLVM 属性や停止性を推定せず、投機実行・永続的 memo cache・所有結果の共有を導入しません。

### 継続の特殊化

型検査および所有権検査が完了した通常の関数呼び出しを対象とし、コンピュテーション式の操作関数名などを特別扱いすることはありません。
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

### 数値

各整数ビット幅、符号の有無、および各浮動小数点形式は、それぞれ厳密に独立した型として扱われます。`byte`／`ubyte` は `i8u`、`sbyte` は `i8` の別名です。
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

浮動小数点・decimal の文字列表現の入口は `tz_soft_format`（書式指定付きの数値は `tz_soft_format_spec`）に統一されており、コンソール出力、`Display`、および `to_string` が完全に同一の出力形式を共有します。整数（`i8`〜`i128`、`i8u`〜`i128u`）の表示は、同じ出力形式を `runtime/integer.ll` の `tz.integer.format`／`tz.integer.format128` が書き、書式指定付きの表示（`+`・`x`・`X`・`o`・`b`。負の数は `-` と絶対値の桁）は `tz.integer.spec`／`tz.integer.spec128` が書きます。10 進の桁、先頭の `-`、最小値（`-128i8`、`-170141183460469231731687303715884105728i128` など）、書式指定の文字列は `tz_soft_format`・`tz_soft_format_spec` と同じで、`tests/integer_format.mjs` が Python の参照と `numeric.ll` に、`tests/size.mjs` がコンパイラ経由で Python と同じ文法の JS の参照に照合します。
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

数値解析（Parse）の入口は UTF-16 の `string` を受け取ります。
整数は `runtime/integer.ll` の `tz.integer.parse`／`tz.integer.parse128` が UTF-16 のコード単位を直接読み、ASCII への複写も 4 KiB の一時バッファも使いません。非 ASCII 文字（U+0130 のように下位バイトだけが数字や `x` と同じ単位を含む）や孤立サロゲートは数字でも記号でもないので、その時点で `None` です。不正な UTF-8 変換や暗黙の文字置換は行いません。
長さは 1〜4096 コード単位、符号は `+`／`-`（符号なしの型の `-` は `-0` も `None`）、基数の接頭辞は小文字の `0x`・`0b` だけで、`0x` の後に数字が要り、`_` は数字と数字の間に 1 つだけ置けます。10 進の先頭の数字の連なりは 19 コード単位まで、`u64` を超えないので範囲の検査なしで足し、和を型の上限と一度だけ比べます。それ以外（`_`、20 コード単位目以降、`0x`・`0b`）は 1 桁ごとに cutoff（上限を基数で割った商）と最終桁（その余り）で判定するループが読むので、64 bit までは `u64` だけで済み、128 bit だけが `u128` の乗除算を使います。
`f16`・`f32`・`f64`・`f128`・decimal の `Parse` は従来どおり、4096 コード単位以下の ASCII 部分を一時バッファに切り出して `tz_soft_parse` が処理します（非 ASCII 文字や孤立サロゲートを含む入力は直ちに `None`）。
`tz_soft_parse` の整数の枝（十進 8 桁を一括して検証する高速経路を含む）は、コンパイラが生成するコードからは呼ばれません。`numeric.ll` を変えない（PM05 が byte 不変を前提にする）ために残してあり、`tests/integer_format.mjs` が `tz.integer.parse` と同じ入力で結果を比べる基準になります。
浮動小数点数の解析では、十進係数と指数を正確な有理数として既存の `pack` 関数へ渡し、目的のバイナリ形式へと 1 回だけ丸め込みます。
係数 4096 桁と形式ごとの指数範囲が `LIMBS` 配列内に安全に収まるよう、極端な指数値は拡大処理の前に overflow または underflow として早期分類されます。
解析失敗時は 0（`Maybe.None`）を返し、成功時のみ出力スロットを初期化します。LLVM は成功分岐でのみスロットの値を読み出し、標準 `Maybe` の Some／None タグと共通のペイロード領域を構築します。
表示用の一時バッファはエントリーブロックの `alloca` による 128 バイトのスタック領域であり、生成された ASCII 結果は `tz.string.from_ascii` を通じて所有権を持つ UTF-16 文字列へと変換されます（`compact` では同じ名前の関数が Latin-1 の文字列を作ります）。

### 文字型

`Type::Char` はすべてのビットパターンが有効値となる LLVM の `i16`、`Type::Utf8Char` は検証済みの Unicode スカラー値を表す `i32` として定義されます。
ソースコード上で異なるリテラル記法で記述された値も、型検査後は既存の整数定数ノードとして保持され、型情報によって区別されます。
値の比較は符号なし整数比較として行われ、`match` 式はそれぞれ `switch i16` または `switch i32` へと lowering されます。安易な整数キャストや C 言語 ABI への直接露出は禁止されています。
`runtime/character.ll` は Display／Parse および最大 4 バイトの UTF-8 出力ロジックを共有し、char 型におけるサロゲート文字の直接出力のみを適切に拒否します。
必要な場合にのみ既存のコンソール／文字列ランタイムとリンクされ、WASM のホストインポートを増やすことはありません。

### 文字列

`Type::String`（UTF-16）と `Type::Utf8String`（UTF-8）は、互いに独立した非 Copy（所有型）の文字列型です。
構文および型付き IR の `StringLiteral::Utf16(Vec<u16>)` は Rust の標準 `String` を経由せずに生データを保持するため、孤立サロゲートを欠落させることなく完全に表現でき、`StringLiteral::Utf8(String)` は検証済みの妥当な UTF-8 バイト列を保持します。`match` の定数パターン比較においても符号単位を失わずに正確に比較されます。
LLVM 上では、`%tz.string` および `%tz.utf8string` という独立した `{ ptr, i64 }` 構造体記述子（ポインタと長さ）が用いられ、長さフィールドはそれぞれ UTF-16 コード単位数、および UTF-8 バイト数を表します（`TSUZURI_STRINGS=compact` の `%tz.string` の長さの語は、bit 62 が Latin-1 の印で、そのとき低位 62 bit が 1 byte の単位の数です。「コンパクト文字列（PM03）」の段落を参照）。
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

### タスク

`Type::Task(T)` は、非 Copy かつコールド（遅延評価）な 1 回実行の並行計算を表現します。
型検査において `task` 式は引数なしのラムダ式として扱われますが、自由変数の捕捉および計算結果に対しては通常の `Capture` 制約ではなく、スレッド間転送を保証する `Send` 制約が課されます。
参照を含むデータ構造や、借用を保持したクロージャ環境をタスク内へ捕捉することは禁止されており、タスク内部で生成された借用参照がタスク外へ漏洩することも防ぎます。
タスク生成時のクロージャ環境は所有権チェッカーによって厳密に検証されるため、持ち上げられた `is_task` ワーカー関数の捕捉引数は完全に閉じられた所有値として安全に取り扱うことができます（通常の関数値引数に対してこのような仮定を置くことはできません）。
`Task<T>` は外部への借用を一切運ばないという不変条件が、抽象本体の段階と具象化後の双方で徹底して維持されます。
通常の再利用可能なクロージャ環境へタスクを誤って捕捉させるようなコードは静的に拒否されます。

LLVM コード生成では関数値と同じ `%tz.closure` と静的記述子の code、environment、drop を再利用しますが、記述子の clone 欄は常に null となり、タスク用環境のクローン関数が生成されることはありません。タスクを含む一時的な完全適用環境もクローン不可となります。
コンピュテーション式における `let!`／`return!` と `Task.run` は、同一の 1 回消費 lowering パイプラインを共有します。
`Task.parallel` 自体は配列の所有権を受け取るコールドタスクを生成し、実行時にのみ結果格納用の連続メモリ領域を確保します。
入力配列の各スロットを 1 回ずつ消費するコールバック関数は、戻り値の LLVM 型ごとに決定論的に重複排除されて生成されます。
コールバック関数の C 言語境界は `(void *context, uint64_t index)` のみに限定されており、集約値の内部 ABI を C 側に露出させることはありません。
すべてのコールバック処理が完了したことを確認した後に元入力配列のメモリ領域が解放され、初期化済みの結果配列が返却されます。
タスクグループのコンテキスト構造体を含め、反復処理で使用されるスタックメモリ（alloca）は関数のエントリーブロックに集約して確保されます。

ネイティブランタイムは `pthread_once` を利用して、`min(sysconf(_SC_NPROCESSORS_ONLN), 32) - 1` 本の常駐ワーカースレッドを遅延起動します。
CPU コア数の取得に失敗した場合は追加ワーカーを起動しません。タスクの要素数が 0 件または 1 件の場合はスレッドプールを起動せず、呼び出し元スレッドがその場でインライン処理します。
プールの mutex は、古い順のグループのリスト、`busy`・`idle`・チャンネルの待ち（F10）、失敗と trap の記録を守ります。ユーザーのコールバックの実行中には保持しません。
グループの次の添字（`next`）は `_Atomic uint64_t` で、mutex なしに `fetch_add` で 1 つずつ取ります（尽きた後は書きません）。`length` は公開後に変わりません。同じ添字が 2 つのスレッドへ渡ることはなく、取った添字は必ず 1 回走ります。
最初の添字は、グループを公開する前に（`next` の初期値を 1 にして）投入したスレッドが取ります（F10）。worker は join のときだけ mutex の下で最初の添字を取り、残りは mutex なしに取り尽くします。チャンネルを待つスレッドが他のグループの未着手の項目を走らせる手伝い（`tz_task_help_one`）も同じ取り方です。投入したスレッドは、自グループが終わるのを待つあいだ、自グループの項目だけを走らせます。
残存数（`remaining`）は、各スレッドが走らせた添字の数を、止まるときに 1 回だけ acq-rel の `fetch_sub` で引きます（添字ごとではありません）。worker は、引くことと `busy`・`idle` の更新を 1 つの lock の中で行うので、投入したスレッドがグループの終わりを見たとき、そのグループの worker は走っているものとして数えられていません。worker が最初に取った添字は、引かれるまで `remaining` を 0 にしないので、スタック上のグループは、それを読む worker の下で消えません。引いた後、スレッドはグループに触れません。
失敗と trap は cold path です。mutex の下で最小の添字を記録し、まだ誰も取っていない添字は `next` を `length` へ進める compare-exchange 1 回で外して、その数を `remaining` から引き、`work_available` を broadcast します。
投入は、`TZ_TASK_INITIAL_SIGNALS`（1）本の signal で眠っている worker を起こします（グループの残りの添字より多くは起こしません）。グループに未着手が残っているうちに最初の添字を取った worker は、未着手の数に応じて `TZ_TASK_CHAIN_SIGNALS`（2）本まで起こします。起きる worker は木の枝のように増え、投入したスレッドが自分で終えるグループでは 1 人しか起きません。全 worker への broadcast はしません。起床が失われない論拠は `tz_task_submit` の直前のコメントにあります。最後に `remaining` を引いたのが worker なら、結合を待つスレッドのために `work_done` を broadcast します（1 グループにつき 1 回以下）。
257 添字のグループで pool の mutex を取る回数は、投入したスレッドが 2 回、参加した worker が 1 回ずつです。`tests/task_runtime.c` が lock・signal・broadcast の数を hook で数えて検査します。
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

### 所有権

A18 の edition 2027 の引数補完は `check_modules_collect` の署名収集で完了します。
解決済みの具体型が `needs_drop` である通常の引数に加え、裸の型変数と関数型も共有 `Type::Reference` にします。
明示の借用・`own`・`mut`・`Task`・`dyn`・SIMD は既定の読み取り補完から除きます。
`const def`・入口の `main`・`extern`・instance のメソッド、明示 region を持つ引数と可変束縛引数は除外します。
生成と所有権検査は edition を見ず、明示の `ref T` と同じ経路を使います。
region 契約を読む前に宣言の引数型にも同じ表面上の補完を反映し、region の推論規則は変更しません。
`CheckedFunction.read_by_default` は診断の補足と editor の補完前の表記に使い、最適化や所有権の判定には使いません。
`Type::Function` の box 化した `FunctionParameters` は物理引数型と `Own`・`Read`・`Mut` の規約を保持します。
全引数が `Own` の metadata は空に正規化し、2026 の型同一性を保ちます。規約は型変数の中へ埋めず、
単相化・部分適用・型の解決で引数リストと一緒に引き継ぎます。読み取り・可変引数の本体と contextual lambda は
参照先の値 view を使います。callback の adapter は既存の Copy・loan 検査を通し、callback 自体の構築は 1 回だけです。
`BORROWING_APIS` は 2027 の Array／List の読み取り入口と Vec の可変入口、および旧版を保存する `_owned` 入口をまとめます。
`Vec.push_mut` は `Vec.push_owned` と異なる builtin identity と LLVM helper を持ち、混在版の ABI を衝突させません。

型検査済みのローカル変数 ID および構造体のフィールドパスに対して、move（移動）と loan（借用）の妥当性が厳密に検証されます。
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

### 配列

`Type::Array` は要素型のみを型情報として保持し、LLVM 上では `%tz.array = { ptr, i64 }`（データポインタと要素数）として表現されます。
`new [T](length, initializer)` および `new [...]` は所有ヒープ領域を動的に確保しますが、`new` を伴わずに変数に束縛された配列リテラルはスタックフレーム領域を使用します（後述の **記憶域** 参照）。いずれの形式で生成された場合であっても、構築完了後に配列の長さや要素を変更することはできません。
初期化関数の式は構築開始時に 1 回だけ評価され、各インデックスに対してその関数値のスナップショットが適用されます。
メモリ確保に先立って、負の要素数、および要素サイズとの乗算における整数オーバーフローが厳格に検査されます。

添字 `xs[i]`、スライスの端、`new [T](length, f)` の長さは、どの整数型の値でも書けます。型検査（`Checker::index_operand`）が、型付き IR で `i64` の式へ正規化するので、`Index`・`Slice`・`NewArray`／`NewList` を読む後段（emitter、`ranges.rs`、定数評価、所有権）は `i64` だけを扱います。
`i64` はそのまま、接尾辞のないリテラルと決まっていない推論変数は `i64` に単一化し、64 bit 以下のほかの型は `Cast` で `i64` にします（`as i64` と同じで、`i64u` はビットをそのまま読むので、`2^63` 以上は負になって境界検査で落ちます）。`xs[i]` と `xs[i as i64]` は同じ型付き IR で、同じコードになります。
`i128`・`i128u` と型変数は、型付き IR の `TypedExprKind::SaturatingIndex`（型は `i64`）で包みます。値を `[i64::MIN, i64::MAX]` へ飽和させる変換で、emitter（`saturating_index`。単相化後の型ごとに、64 bit 未満は `sext`／`zext`、`i64u` と 128 bit は `icmp` と `select` の後に `trunc`）、定数評価（`const_machine.rs`）、`const def` の検査（`const_rules.rs`）がそれぞれ扱います。組み込み関数の呼び出しにしないのは、`const def` の中で使えるようにするためと、ホバーが添字の式の型を示すようにするためです（`semantic.rs` は、オペランドと同じ span の `Cast` と `SaturatingIndex` を候補に入れません）。`i64` に収まらない値は範囲外の `i64` になるので、下位ビットへ折り返して別の要素を読むことはありません。
境界検査の省略（`ranges.rs`）が添字として認めるのは `i64` のローカル変数と整数リテラルなので、ほかの型の添字の検査は残ります。
ヒープに確保される空配列、およびサイズ 0 の要素型を持つ配列であっても、確保バイト数は最低 1 バイト以上が要求され、プラットフォーム依存の `malloc(0)` の挙動に依存しない安全な設計となっています。
`new` を伴わない空配列リテラルはメモリ実体を持たない `{ null, 0 }` として表現され、その解放処理は安全な no-op となります。
配列の Copy 操作やクロージャ環境の複製時には内部バッファが独立してディープコピーされ、要素型の clone や drop も反復して実行されます。
要素型がプリミティブなスカラー型であっても配列バッファ自体のメモリ解放が必要となるため、すべての配列型において `needs_drop` は真となります。
長さの取得、インデックス読み出し、およびスライス借用において配列全体が無駄に複製されることはなく、要素の読み出しに必要な最小限の複製と一時値の解放のみが行われます。
単相化や型クラスのインスタンス選択に配列の長さは関与しません。再帰 union の要素配列は、前述のスタックレス反復走査機構へと登録されます。

### 連結リスト

`Type::List` は構文上の `[|T|]` に対応し、LLVM 上ではリスト記述子 `%tz.list = { ptr, i64 }`（先頭の chunk へのポインターと長さ。空リストは `zeroinitializer`）として表現されます（PM04）。chunk は要素ごとの構造体 `{ ptr next, i64 rest, i64 capacity, [0 x T] slots }` を 1 回で確保したもので、header は native・wasm32・wasm64 のどれでも 24 bytes、`slots` はその後ろに `T` のアライメントで並びます。この配置を知るのは `src/llvm_list.rs` だけで、ほかの emitter はその helper（一括生成の `list_builder`、走査の `list_loop`、索引の `list_locate`、tail の `list_skip`、`List.cons`・`List.tail` の `list_push_front`・`list_pop_front`、解放と複製、スタックのリテラル）を呼びます。
リスト `(c, len)` の順は、chunk の中ではアドレスの順で、chunk は先頭から順につながります。不変条件は次のとおりです。`len == 0` と `c == null` は同値です。先頭の chunk が持つ要素は `live = len - c.rest` 個で、最後の `live` 個の slot（最初の要素は slot `c.capacity - live`）です。それより前の slot は空きで、`List.cons` が下へ向かって埋めます。先頭以外の chunk はすべて満杯で、次の chunk の最初の要素は slot 0 です。`next` と `rest` は chunk を作った後は変わらず、`capacity` が変わるのはループの中のリテラルの組み立て中だけです（下）。リストの suffix（`List.tail` の結果とパターンの tail）は同じ chunk に短い長さを組み合わせたものなので、chunk の中で進めるのに費用はかかりません。
`new [|...|]`、`new [|T|](length, initializer)`、`Array.to_list`、`List.map`・`mapr`・`rev`、複製、スタックからヒープへの移送のように一度に作るリストは、要素数ちょうどの 1 つの chunk です（確保は 1 回で、空リストは確保しません）。`new [|T|](length, initializer)` の長さの検査は、要素を計算する前に、要素と次のポインターを持つ従来のノード `{ ptr, T }` の大きさで行うので、`AllocationSize` の trap の境界は変わりません。`List.cons` は、先頭の chunk に空きがあればその slot に書き、満杯のときは新しい先頭の chunk を足します。その容量は、最初が header 込みで 32 bytes に収まる数で最低 1 つ（`i64` は 1、`i32` は 2、`i8` は 8。16 bytes の要素は 1 slot で chunk は 40 bytes、`i128` と 24 bytes の record は 48 bytes になり、32 bytes に収まるのは要素が 8 bytes 以下のときです）、次からは今の先頭の容量の 1.5 倍に 1 を足した数で、header 込みで 32 KiB（`i64` は 4,093 slot。1 つの要素が 32,744 bytes を超えるときは、1 slot の chunk がそれを超えます）が上限です。`List.tail` は先頭要素を drop し、先頭の chunk は最後の要素と一緒に解放します。
空リストの先頭ポインターは NULL であり、確保は発生しません。`.length` の取得は $O(1)$ で、添字アクセスは範囲検査の後に、その添字より前の chunk の数だけ `next` を辿ります（一度に作ったリストは $O(1)$、`List.cons` で伸ばしたリストは 100 万要素で約 260 chunk）。走査（`for`、`List.foldr`、複製、hash、表示など）は chunk ごとの二重 loop です。外側の loop は chunk を順にたどり（`next` を読むのは、その chunk の live な slot を数え終えて、まだ要素が残っているときだけです）、内側の loop は chunk の最初の live な slot から `run` 個の slot を 0 から数える loop で、要素のアドレスは `getelementptr inbounds T, ptr base, i64 offset` です。内側の loop は配列の loop と同じ形なので、LLVM が unroll とベクトル化をします（`for` の総和は、node の連結の約 1.2 ns / 要素に対して約 0.12 ns / 要素。値は `docs/benchmarks.md`）。2 つのリストの構造比較は、左右の現在の chunk に残る slot の数と残りの組数の最小を run として、run ごとに内側の loop を回し、run で chunk を使い切った側だけが次の chunk に進みます。比較は要素ごとに早く抜けるので、内側の loop はベクトル化されません。
構造的な Copy やクロージャ環境の複製では、要素が元の順に clone されて 1 つの chunk に入り（要素が drop 不要のときは、clone も `List.to_array` と `Array.to_list` も、元の chunk の live な slot ごとの `llvm.memcpy` です。intrinsic は `llvm.memcpy.p0.p0.i64`、wasm32 は `.i32`）、すべてのリスト型において `needs_drop` は真となります。drop は chunk ごとの外側の loop と、その chunk の live な slot を順に drop する内側の loop で、chunk の `next` を読んでから free します。要素が drop 不要のリストは slot を drop せず、最初の chunk の解放を loop の前に出します（LLVM は作ったばかりの chunk の `next` をそれを書いた store から知るので、同じ関数で作って捨てるリストの確保と解放が消えます）。ただし、要素数が定数の小さなリストを同じ関数の中で作って使って捨てるとき、node の連結では LLVM が確保を全部消した（1 回約 1 ns）のに対し、chunk では `new` のリテラルを走査するだけの場合を除き、`List.map`・`List.rev`・`List.copy`・`List.cons` の連鎖・要素が文字列のリストに 1〜3 個の chunk の確保が残り、1 回約 25〜80 ns かかります（既知の制限。要素数が実行時に決まるリストは速くなります。数値と原因は `docs/benchmarks.md`）。clone、drop、および走査は反復ループによってスタックレスに処理されます。ループまたは `try` の中で評価されるヒープのリテラルが、途中の `break`・`continue`・例外で抜ける場合は、それまでに格納した要素の数まで `capacity` を縮めた chunk（最初の要素の前に抜けるときは、要素が 0 個で chunk だけ）を一時値に登録して drop し、組み立てが終わると `capacity` を要素数に戻します（空きの slot が先頭側にあるので、縮めた chunk の live な slot は slot 0 から始まります）。
リストのリテラルのスタック上の形は、header を持つ 1 つの `alloca { ptr, i64, i64, [n x T] }`（`align 16`。header は `next = null`、`rest = 0`、`capacity = n`）で、エントリーブロックに置かれるので、非常に長いリストの生成や末尾再帰の実行時にもコールスタックを累積消費しません。
配列と同様の多相化、所有権、借用追跡、および ABI の制限が適用されます。

### 記憶域（スタックとヒープ）

`new [...]` や `new [|...|]` は構文 AST および型付き IR において `NewLiteral` でラップされますが、型および所有権の規則は内側のリテラルと完全に同一です。
値がスタックに配置されるかヒープに配置されるかは、型システムではなく LLVM emitter が値の生成コンテキストに基づいて静的に決定し、`Type`、型クラス、単相化、および ABI のレベルには現れません。
`let` の初期化式、`match` の対象式、`for` ループの列挙元、ならびに添字アクセス・長さ取得・文字列比較・文字列連結の被演算子に出現するリテラルの構文木（`if` や `match`、ブロック式の結果、およびレコード・タプル・コレクションのリテラル要素を経由したものを含む）は、`frame_value` によってスタック上に構築されます。
配列は `[N x T]`、リストは 1 つの完全な chunk `{ ptr, i64, i64, [N x T] }`（header を書いた状態。PM04）のエントリーブロック `alloca`、文字列リテラルはプライベート定数領域を直接参照し、空のコレクションは `{ null, 0 }` となります。
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
関数ごとの合計にも 64 KiB の上限（`MAX_FRAME_BYTES`、`check.rs`）があります。`FunctionEmitter.frame_bytes_used`（スタックに置くと決めたリテラル木の見積もりの合計）と `buffer_bytes`（`reserve_buffer` が取った文字列連結のバッファとクロージャ環境）の和が上限を超える木は、1 つが `MAX_VALUE_BYTES` 以下でもヒープに置きます。`frame_value` は見積もりを 1 回だけ計算し、スタックに置くときだけ `frame_bytes_used` に足します。バッファと環境の合計 4 KiB（`MAX_FUNCTION_BUFFER_BYTES`）と、再帰の循環に入る関数・Task の本体・ヒープ初期化子への不使用は変わりません。
スタックに置いた配列とリストの領域（`Frame::Array`・`Frame::List`）には、寿命の印を付けます。`llvm.lifetime.start.p0` は最初の store の前（配列は `alloca` の直後、リストは header の store の前）に、`llvm.lifetime.end.p0` は `drop_framed` の中で、`frame_test` が真（値がまだそのフレーム領域を使っている）の枝の `drop_frame_contents` の後にだけ出します。ヒープへ移送した値と、ムーブ済みの領域には `end` を出しません。印の行は `!tz.site` と `!dbg` を持たず（`lifetime` が `self.lines` へ直接積みます）、トラップの地点を使いません。大きさは対象のレイアウトの正確な値（`i128`・`Decimal128`・`Binary128`・SIMD のベクトルに届く型は、`target_layout` で確定できないので印を付けません。`layout_bound` の下限は、wasm32 で 16 byte のベクトルを 16 に整列する LLVM に対して 8 としか見積もらず、`alloca` より小さい印になるためです。`tests/control.mjs` の `checkMarkerSizes` が、`control` と `simd` の native と wasm32 の IR で、印の大きさを clang の `sizeof` と照合します）で、ASan が `start` の大きさだけを unpoison するため、下限値や `-1` は使えません。宣言 `declare void @llvm.lifetime.{start,end}.p0(i64 immarg, ptr)` は `intrinsics` で 1 回だけ出し、ランタイム IR との結合で重複しないよう、`numeric.ll` の前に生成された宣言を取り除きます。`-O1` 以上の LLVM は、この印で同時に生きていない領域を同じスタック位置に重ねます（インライン展開した関数の `alloca` には、印がなければ LLVM が自分で関数全体の印を付けるので、効果は再帰する関数のように展開されない関数に出ます）。`-O0` は領域を重ねません。文字列連結のバッファ（`Frame::Buffer`）とクロージャ環境（`Frame::Environment`）の `alloca` には印を付けません。印のある `alloca` には LLVM のインライナーが自分の印を付けず、この 2 つは `end` のない経路（値がヒープへ移った経路）で解放されるので、展開された呼び出し元のフレームがかえって大きくなりました（`-O3` の合計は 108 のプロジェクトで +23,872 バイト。配列とリストだけで、下の分岐の枝の領域を除いた版は −464 バイトで、大きくなるプロジェクトはありません）。同じ値が持ちうる複数のフレーム（`emit_frame_inner` の `if` の両方の枝と、複数の arm を持つ `match`（`match_expression` の `delivered`）の arm が作るフレーム）と、`let mut` の束縛の初期化子が作るフレームにも、印を付けません（`unmarked_frames` が 0 でないあいだ、`slots_bytes` は `None` を返します）。`frame_test` は値のアドレスを候補のフレームのアドレスと比べるので、LLVM のスタックの色分け（`StackColoring`）が寿命の重ならない 2 つの `alloca`（別の枝の領域）に同じ場所を与えると、先の候補の比較が、後の候補を使う値でも真になるためです。IR の段階の最適化は、別々の `alloca` のアドレスが等しくないと見なして、その枝の中身を畳むので、`string_library` の `validate_bytes` の `[128]` が wasm32 の `-O3` で `[0]` として読まれ、`1` を返しました（印を付けていた版）。`let mut` は、代入が値を `drop_slot` で早く解放して `end` を出すので、あとから別の領域と同じ候補の組に入り得て、同じ扱いです。候補が 1 つの領域は、比べる相手がヒープかヌルだけなので、印を付けたままです。`tests/stack_frames.rs` の `leaves_the_frames_that_one_value_may_hold_without_markers` と、印があると WASM の `-O3` で失敗する `tests/control.mjs` の `frames_validate` が確かめます。測定は [benchmarks.md](benchmarks.md#スタックとフレームの大きさ) にあります。
変数に束縛されず直接返却される戻り値や実引数の位置に書かれたリテラルは、無駄なスタック配置と移送をスキップして直接ヒープ上に構築されます。
なお、関数値のクロージャ環境およびタスクはこのフレーム最適化の対象外であり、既存の特殊化機構（既知のエスケープしない呼び出しに対するスタック環境割り当て）が適用されます。

### コレクションの不変性

2026 では配列・リストの要素代入構文を禁止します。2027 では可変な所有者または排他参照を根とする
record の field、配列・リスト・Vec・固定長配列の要素へ代入でき、既存の `Assign` と place の経路を使います。
RHS、代入先、古い値の drop、store の順で評価し、添字は既存の範囲検査を通します。
移動済みの射影の再初期化、共有 loan と重なる書き込み、自己参照の格納は拒否します。
射影への store は新旧の loan を weak update で合わせ、兄弟 field の loan を消しません。
`Drop.drop` の自己の field・要素への代入は、固定プロトコルの禁止を ownership 検査で維持します。
LLVM の射影 store は `frame_of_store_place` で field と indexed element の frame 候補を引き継ぎ、
導出した slot の古い値を `drop_framed` で解放します。動的な添字では元の各要素の候補を保守的に合わせます。
この経路は新しい Field／Index の代入だけに適用し、従来の局所・参照先の代入の IR は変えません。
文字列・UTF-8 文字列の要素と、要素単位の排他借用（`ref mut xs[i]`）は引き続き許しません。
要素型から入れ子のフィールドや共有参照を辿って可変参照に到達できるような型定義も `validate_size` によって厳格に拒否され、単相化後のすべての具象型に対しても再検査が行われます（なお、関数シグネチャに含まれる可変参照はデータを格納する状態ではないため対象外であり、排他参照のクロージャ捕捉は既存の Capture 検査で拒否されます）。共有借用された要素の生存期間は通常の借用（loan）追跡によって安全に保護されます。
可変変数（`let mut`）や `&mut` 参照を通じてコレクション全体を丸ごと新しいコレクションで置き換える操作は、従来どおり許可されます。

### LLVM

整数の算術命令に `nsw`（No Signed Wrap）や `nuw`（No Unsigned Wrap）の未定義動作フラグを付与するのは、範囲証明がその向きのオーバーフローを否定できた命令だけです。既定の `off` と `audit` は 1 つも書きません（「整数の範囲証明」）。除算および剰余算ではゼロ除算と符号付きの `MIN / -1` を明示的に検査し、ビットシフト演算ではシフト量をビット幅に合わせてマスク処理し、浮動小数点数から整数への変換には飽和変換 intrinsic（`llvm.fptosi.sat`／`llvm.fptoui.sat`）を用いて poison や範囲外による未定義動作を徹底的に排除します。
`@checked` ブロック内部に記述された整数の `+`、`-`、`*`、`**`、および単項 `-` のみは、`Int.checked_*` と同一の `checked_integer_arithmetic` によってオーバーフローが検査され、オーバーフロー時には `OverflowException` を送出します。整数のべき乗 `**` は `Int.wrapping_pow` と同様に二乗法と乗算の反復（符号付きの負の指数は `NumericRuntime` のトラップ）で計算され、`f32`／`f64` の `**` は `Math.pow` と同一の `tz_math_pow_f*` を呼び出します。
`Int.wrapping_add`／`wrapping_sub`／`wrapping_mul`／`wrapping_neg`（A23）は旗のない `add`・`sub`・`mul`・`sub 0` の 1 命令です。組み込み関数の参照はすべて `$builtin` のラッパーを呼ぶので、`FunctionEmitter::call` はラッパーの直接の呼び出し（`call_specialization::wrapping_builtin`）とラッパー自身の本体でこの命令を inline に書き（`Add.add` の PR02 D10 と同じ）、`-O0` でも演算子と同じ命令になります。
例外は 32-bit 整数のエラーコードとして表現され、同一関数本体内の最も内側に位置する `try` のハンドラへと、途中のスコープの一時値を安全に解放しながら分岐します（`try_targets`）。
例外が関数やラムダ式の境界を越えて自動伝播することはありません。外側を囲む `try` ブロックが存在しない場合は `TrapKind::Overflow` として安全にトラップします。`finally` ブロックは、ハンドラ内部から送出された例外を含むすべての実行経路で確実に 1 回実行されます。
i8〜i64／i8u〜i64u から f32／f64 への変換には LLVM の `sitofp`／`uitofp` を用い、f32 と f64 の相互変換には `fpext`／`fptrunc` を使用します。i128、f128、および decimal 型を含む変換と、整数と f16 の間の変換は正確なソフトウェア演算経路を維持し、f64 を経由することで二重丸め誤差を生じさせることはありません。f16 と f32／f64 の間の変換は、f16 の経路が `Conversions` か `Hardware` のとき（後述）だけ、`half` への `fpext`／`fptrunc` になります。
intrinsic の外部宣言は組み込み関数と通常の式で共通化され、重複なく決定論的な順序で出力されます。連結したランタイム IR（`numeric.ll`・`math.ll` など）が同じ関数を宣言するときも、`emit_program` の最後の `unique_declarations` が、シンボル名ごとに最初の `declare` だけを残します。LLVM は属性グループだけが違う 2 つ目の `declare` も `invalid redefinition of function` として拒否するので、行全体ではなくシンボル名（`@` から `(` まで）で比べます。生成コードの宣言はランタイム IR の本文より前に書かれるため生成コードの宣言が残り、重複の無い IR は入力のまま（byte 単位で同一）です。`numeric.ll` の宣言と重なる `llvm.smin.i32`・`llvm.smax.i32`・`llvm.ctlz.i32` がその例です。`llvm::duplicate_declarations` は 2 回以上宣言されたシンボルを返し、テストの IR 補助関数と、debug build の driver が書き出す前に行う `debug_assert!` が、重複の無いことを確かめます。
負の添字アクセスを含め、配列やリストの境界検査は要素の GEP／load や chunk の走査よりも前に先行して実行されます。
浮動小数点演算に対して fast-math フラグを安易に付与することはありません。
借用可能な値のメモリ領域は関数のエントリーブロックに集約して配置されます。所有値を move すると移動元のメモリ領域は即座にゼロクリアされ、スコープ終了時に残存する文字列、配列、リスト、およびクロージャ環境が確実に解放されます（レコードの部分 move も同一の機構に従います）。
スタックフレームの領域を指している可能性のあるスロットの解放や、再代入前の解放処理においては、前述のアドレス判定ロジックによりフレーム領域を誤って `free` しないよう保護されています。

### 末尾再帰

直接の自己末尾呼び出し（self tail call）は、実引数の評価が完了した後にループの先頭へと分岐し、すべての引数を phi ノードのバックエッジによって同時に更新します。
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
反復を開始する前に未移動の所有値は確実に解放されます。なお、借用を含む引数が存在する場合はフレーム領域の再利用は行われません。例外は、リフトした `let rec` 関数（`capture_count` が 0 でないもの）が、借用して捕捉した値の引数に自分の同じ引数をそのまま渡す自己末尾呼び出しです。参照先が自分のフレームの外にあるので、`is_self` は引数を見て、この呼び出しをループにします。

#### 保証された末尾呼び出し（`musttail`、PM09）

同じ直接呼び出しグループ（`Globals.tail_groups`）のほかの関数への末尾呼び出しは、`FunctionEmitter::guaranteed_tail` が判定し、満たせば `emit_tail` の 2 つの arm（`TypedExprKind::Call` と 1 引数の `BinaryOp::Pipe`。自己呼び出しの arm より後）が `guaranteed_call` を呼びます。
- `tail_groups` は `llvm_escape.rs` の `edges(module, closures)` から閉包の辺を除いた直接参照のグラフ（`FunctionRef::User(id)`）の強連結成分で、`recursive_functions` と同じ反復の `components`（Kosaraju。Rust のスタックで再帰しない）を使います。成分の番号は最小の関数 id で、結果は決定的です。`Globals.tail_calls` が偽のときは計算しません。
- `tail_calls` は native では常に真で、WASM では `--wasm-feature tail-call`（`Instrumentation.wasm_tail_call`、driver から `emit_wasm_build` の引数）のときだけ真です。偽のとき `guaranteed_tail` は `None` を返し、IR は変わりません。
- 条件は、呼び出し先が名前で呼ぶ `FunctionRef::User`、自分でなく（自己呼び出しはループ）、同じ成分で、全引数を渡し、2 つの関数の引数の LLVM の型が同じで、どちらも `capture_count == 0`・Task の本体でなく、`@cpu` の版分けの対象（`multiversion` のとき）でなく、`@tz.specialized.` の関数や借用ワーカーの中ではなく、引数にも呼び出し先の結果にも `carries_loans` の型がなく、2 つの結果の**言語の型**が `escape::guaranteed_result` を満たすことです。`guaranteed_result` は `Bool`・`Unit`・`Char`・`Utf8Char`と、幅が 64 bit 以下の `Integer`・`Binary`・`Decimal` だけを通します。LLVM の型で決めると、表現が `ptr` になる結果（再帰する共用体、`Rc`・`Arc`・`extern type`）まで通ってしまい、型から予測できず、文書の「共用体の結果は対象外」と食い違うので、LLVM の型は見ません（通した型が `i1`・`i8`・`i16`・`i32`・`i64`・`float`・`double` のどれかであることを `debug_assert!` で確かめます）。aggregate の結果は、ターゲットによってメモリ（sret）で返り、呼び出し元のフレームへ書くことになりうるので、対象にしません。`tests/tail_recursion.rs` の `guarantees_the_results_that_the_language_type_names_and_no_other` が、通す型（`bool`・`unit`・`char`・`utf8char`・8〜64 bit の符号つき・なしの整数・`f16`・`f32`・`f64`・`d32`・`d64`）と、通さない型（`i128`・`f128`・`d128`・文字列・配列・リスト・タプル・SIMD・レコード・enum と payload つきと niche と再帰する共用体・`Arc`・`Maybe<Arc<_>>`）を固定し、通さない型の呼び出しでは所有値が呼び出しの後に解放されることも確かめます。
- `guaranteed_call` は、引数を左から右へ `self.expression` で 1 回ずつ評価し、`drop_all()` で呼び出し元が持つ所有値を解放し、`self.value` で `musttail call` を、`self.instruction` で直後の `ret` を出します。間に別の行を置かないので、`lifetime`・drop・debug の行は call と ret の間に入りません。自己呼び出しのループが引数の後に drop するのと同じ順序で、通常の呼び出しの「戻ってから drop」とは違い、`Drop` の副作用の順序が変わります。
- `--trap-info` では、`traps::instrument` が管理される関数の定義の末尾に `i32 %tz.context` を足し、呼び出しに `i32 <定数|%tz.context>` を足します。`callable(line, false)` は `call ` を探すので `musttail call` の行も同じに扱われ、呼び出し元と呼び出し先の prototype は一致し、`ret` は直後に残ります（`tests/tail_recursion.rs` の `native_trap_context_and_debug_builds_keep_the_prototypes_equal`）。
- LLVM は、`alwaysinline` の関数（lambda の worker など）に `musttail` 呼び出しがあっても、末尾位置でない呼び出しにインライン展開するときは `musttail` を外すので、条件には入れません。
- `--backend fast`（`src/backend/`）は、`parse.rs` が `musttail` を `Inst::Call.must_tail` として読み、`Module::tail_functions`（`module.rs`）が内部で、可変長引数でなく、`byval`・`sret` を持たず、アドレスを取られない関数の集合を決め、`abi::plan_tail` が `CallConv::Tail` の署名を作り（16 個を超える葉の集成体の引数と、メモリ越しの結果は計画できず、その関数は通常の規約のまま）、`lower.rs` が `musttail` の call の直後が `ret` のとき `call.rs` の `tail_call` で `return_call` にします。計画できない組の `musttail` は、通常の call と ret のままです。wasm32 の翻訳器は `musttail` を通常の call として読みます。そのため `validate_fast` が wasm32 の `--wasm-feature tail-call` を `E2000` で拒否します。
- WASM の clang 引数は、既定で `-mno-tail-call`、`--wasm-feature tail-call` で `-mtail-call` です。ランタイムの object（`numeric`・`math`）の引数は、同梱の object の一致のため変えません。`wasm-ld` は tail call を使う object とそれらを混ぜてリンクできます。
- 検証は `tests/tail_recursion.rs`（IR）、`tests/stack.mjs`（深さ 10,000,000 の実行と、WASM の既定の出力）、`tests/fast_backend.rs` と `tests/fast_backend_ops.rs`（`return_call` の翻訳と、Clang との比較）、`tests/user_drop.mjs`（drop の順序）、`tests/control.mjs`（浅い循環と、保証の対象外の形）です。

### WASM

bulk-memory 機能を標準で有効化し、集約値のメモリコピーにおいてホスト環境の `memcpy` インポートを要求しません。
LLVM の閉形式ループ最適化（closed-form loop optimization）は、`i64` のプログラムに対しても内部的に `i128` の乗除算命令を導入することがあります。
同梱のランタイムでは、乗算を i64／32-bit limb、除算を固定ビットシフトと減算の組み合わせで自前実装し、その補助関数自身が wide multiply/divide の libcall 呼び出しへと再帰的に再変換されて無限ループに陥る循環を防止しています。
LLVM 23 は limb の乗算式を再び i128 乗算としてパターン認識してしまうため、乗算補助関数には `noinline optnone` 属性を付与して InstCombine パスから保護しています（可変シフトの limb 補助関数も同様に保護されています）。
なお、アプリケーション関数に対する通常のコード最適化は完全に維持されます。
weak かつ hidden な補助関数シンボルはオブジェクト間での重複が許容され、未使用のものは `wasm-ld` のデッドコード削除によって最終バイナリからきれいに除去されます。
WASM ヒープ（`heap-wasm.ll`・`heap-wasm64.ll`）は、16 バイトのヘッダーと 16 バイト境界のペイロードからなるブロックを、`__heap_base`（リンカーが 16 に揃えます）から `@tz.heap.end` まで隙間なく並べます。ヘッダーの先頭の語は容量（16 の倍数で、ヘッダーを含めて 32 以上）と、FREE・PREV_FREE の 2 つのフラグです。空きブロックは次と前のリンクを持ち、末尾に容量のフッターを置くので、解放は前後の空きブロックを定数時間で結合します。容量 32〜528 の空きブロックは容量ごとの 32 本のリスト（後入れ先出し）にあり、ビットマスクと `cttz` で、要求を満たす最小のリストを 1 命令で選びます。それより大きい空きブロックは 1 本のアドレス順のリストにあり、最初に収まるものを使います（走査は大きい空きブロックの数に比例します）。ただし、64 KiB 以上で、しかも要求の 4 倍以上の残りを切り出すことになるブロックは飛ばします。切り刻んだ穴は、次の大きな要求の穴ではなくなるからです。飛ばすのは、最近の最大の要求（`@tz.heap.largest`。大きいブロックのリストを探した要求の容量の最大で、`@tz.heap.alloc_slow` が探すたびに 1/64 を引いて更新します）の 2 倍までのブロックだけです。それより大きい穴は、最近のどの要求も近づいていなくて、取っておくとヒープが上限まで伸びるので、普通のブロックと同じに切り出します。`@tz.heap.alloc_slow` は、飛ばした最初のブロックを覚えながらリストを先へ進み、そう切り出さずに済む最初のブロックがあればそれを使います。なければ、終端を延ばせる間は終端から返し、延ばせないとき（上限の手前）は、覚えておいた最初のブロックから切り出します（飛ばしたブロックしかないときは、リストの最後まで見ます）。穴を残して終端を延ばすので、大きな一時配列と長く生きる小さなブロックが混ざるプログラムでは、生きているデータより大きなヒープを使うことがあります。
解放した容量 32〜528 のブロックは、すぐには結合せず、容量ごとのキャッシュ（`@tz.heap.fast`、後入れ先出し、ペイロード先頭の語でつなぎます）に積みます。キャッシュのブロックはほかのブロックから見れば使用中のままで、同じ容量の要求には数命令で取り出して返します。ほかのリストが要求を満たせずヒープを伸ばすしかないとき、またブロックが伸びたいのに隣がキャッシュのブロックのときに、キャッシュ全体を本当に解放します（`@tz.heap.flush`。前後の空きブロックと結合し、終端に触れたブロックは `@tz.heap.end` を下げて返します）。`@tz.alloc` と `@tz.free` はキャッシュの出し入れだけの小さな関数で、インライン化されます。残りは `@tz.heap.alloc_slow` と `@tz.heap.release` が out of line で担います。
`@tz.realloc` は、容量が足りていればそのまま返し、終端のブロックは `@tz.heap.extend`（上限の検査と `memory.grow`）で終端を延ばしてその場で伸ばします。隣が空きなら吸収し、それでも足りなければ新しいブロックを確保して `memory.copy`（`llvm.memcpy`）で複製し、元のブロックを解放します。0 バイトの要求も容量 32 のブロックを取ります。
`memory.grow` の失敗やメモリ上限（デフォルト 16 MiB）の超過時には安全にトラップを発生させ、見かけ上成功したかのような無効ポインタを返却することはありません。
ヒープ上限およびメインスタックサイズは、`BuildOptions` および `TestOptions` の `wasm_max_memory` と `wasm_stack_size`（CLI オプション `--wasm-max-memory`、`--wasm-stack-size`）によって設定可能です。
プロジェクト読み込み後、未指定のオプションはルートパッケージのマニフェスト内 `[wasm]` セクション（`Project::wasm`、`BuildOptions::with_manifest_wasm`）から補填され、適用後に妥当性が再検証されます。
`driver::wasm_memory_limits` がターゲットごとのメモリ範囲（wasm32 は 4 GiB − 64 KiB、wasm64 は 16 GiB）を一元的に検査して実効値を算出し、ビルドおよびテストランナーはその実効値から `wasm-ld` の `-z stack-size=` および `--max-memory=` 引数を組み立てます。
`--wasm-opt`（PM08）は、`wasm-ld` が終わったあとの `.wasm` を `driver::run_wasm_opt` が Binaryen の `wasm-opt -Oz`（機能は `bulk-memory`・`sign-ext`・`nontrapping-float-to-int`・`mutable-globals`・`multivalue`・`reference-types`、`--wasm-feature simd128` のときだけ `simd`、`--wasm-feature tail-call` のときだけ `tail-call`）に通し、結果で成果物を置き換えます。`--wasm-opt` を付けたときだけ `TSUZURI_WASM_OPT`（既定 `wasm-opt`）を探し、見つからないか失敗すると `run_tool` が `E2002` とヒントを返します。成果物キャッシュのキー（`cache::build_key`）は、そのときだけ `wasm-opt` の実体を含みます。`wasm64`・threads・`-g` は `BuildOptions::validate` が `E2000` にします。
プロファイル誘導最適化（PR07）は `src/profile.rs` が担い、`driver::build_complete` からは 4 か所で呼びます。①先頭の `Profile::validate`（出力の許可リスト）と同梱の launcher の `E2002`。②キャッシュの前の `prepare_profile`（`.profraw` のディレクトリは `llvm-profdata merge` をファイル名の順に実行して一時ディレクトリの `profile.profdata` にする。path は引数でなく、同じ一時ディレクトリの一覧のファイル `--input-files`（1 行 `1,<path>`）で渡す。ファイルは複製して magic を確かめる。Clang が読むのはこの複製）。③`cache::build_key` の `profile` の引数（`Some` のときだけ `profile-use-sha256`（複製の SHA-256）か `profile-generate-dir` を `ir-and-embedded-runtime` の直後に足すので、旗なしのキーは変わらない）。④Clang の `-x ir` の起動の `-O<n>` の直後に `-fprofile-generate=DIR`（wasm32 は `-fprofile-generate` に `-mllvm -disable-vp`）か `-fprofile-use=<複製>` を足す（macOS の `-g` の別リンクにも）。Tsuzuri の IR は変えません。計装した Clang の出力は、`ld` の整列の警告 3 行（x86_64 の macOS で、Apple 以外の Clang が計装のリンクに `-sectalign __DATA __llvm_prf_{cnts,bits,data} 0x4000` を足すため）だけを除いて `messages` に入れ、ほかの行と警告は残します。wasm32 の計装は、IR に `@__llvm_profile_runtime = hidden global i32 0` を足して Clang のランタイムを要らなくし、`wasm-ld` に `__start___llvm_prf_*`・`__stop___llvm_prf_*`・`__llvm_profile_raw_version`・メモリを export させ、`src/runtime/profile-wasm.mjs`（DIR を既定値に埋める。`writeProfile` は `default_wasm_<pid>_<n>.profraw` を排他的に作り、使われている名前は飛ばすので、module や Worker が何個あっても profile は上書きされない。ファイルに入るのは前に呼んだあとの分だけで、書き終えるとその instance のカウンターだけを 0 に戻し（data と names は残す。失敗したら数えた分を戻す）、1 つの instance が書いた何個のファイルも merge すると、数えた分がちょうど 1 回ずつになる。アドレスは `>>> 0` の符号なしで読むので、スタックが 2 GiB 以上で section が 2^31 より上にある module も読める）を `<output>.profile.mjs` として出します（成果物キャッシュの名前は `profile`）。`run` は同じ `Profile` を `build_complete` へ渡し、正常に終了したのに raw profile が書かれなかったら `W2001` を 1 行足します。`LLVM_PROFILE_FILE` と `TSUZURI_LLVM_PROFDATA` はキーに入れません（前者は実行時だけに効き、後者の結果は merge 済みの中身がキーに入る）。数値・数学の prebuilt object（PB01）と C ランタイムは、計装も使用もしません。数学関数の例で、埋め込んで計装しても差が測れなかったためです。
ヒープランタイム側の `heap-wasm.ll` は直接改変せず、`llvm::with_wasm_heap_limit` が生成後の IR 内にある `%fits` の 2 行と `%within` の 1 行を行全体の一致判定で安全に置換します。`%within` の行は `@tz.heap.extend` にあり、`@tz.alloc` の終端への追加と `@tz.realloc` の終端での伸長が共有します。
ユーザー定義の文字列定数は `c"..."` の行の途中に出現するため影響を受けず、デフォルト値の環境では置換を行わないため、デフォルト設定時の IR および生成される `.wasm` バイナリは完全に不変です。
上限が 2 GiB 以下であれば `i32` の終端計算 `%begin + %needed` はオーバーフローしません。2 GiB を超過する場合は、`%within` を `%room = sub i32 LIMIT, %begin` との比較を行う 2 行の命令へと置き換えます（`%begin` は上限を超えません）。
上限を 4 GiB − 64 KiB に制限しているのは、アライメント計算 `%ceil = add i32 %end, 65535` がオーバーフローするのを防ぐためです。JavaScript の同梱ホストはポインタを符号なし 32-bit 整数（`>>> 0`）として受領します。
wasm64 ターゲットでは `heap-wasm64.ll`（同じ設計で、ヘッダーの語・リンク・フッターは 8 バイト、`memory.size`/`grow` と `cttz` は i64）を使用し、同様の 3 行置換によってメモリ上限を設定します。
`Instrumentation::memory64` は、ホスト ABI のメモリ範囲検査、スカラー捕捉のビット幅、および DWARF デバッグ情報のポインタ幅をすべて 64-bit に設定し、Clang には `--target=wasm64-unknown-unknown`、`wasm-ld` には `-mwasm64` を指定します（なお、スレッド機能 threads は現在のところ wasm32 のみ対応です）。
スタックは枯渇するとアドレス 0 を下回って循環（ラップアラウンド）します。wasm32 で 2 GiB を超えるメモリ空間や、ワーカースレッドのスタックをヒープブロック上に確保する threads 環境では、ラップアラウンド先が正常なヒープメモリ領域と重なってメモリ破壊を引き起こす恐れがあります。これを防ぐため、`llvm::with_stack_checks` が全関数のエントリーブロックにおける静的 alloca の直後にスタック境界検査 `@tz.stack.check` を自動挿入します。
alloca 命令を関数呼び出しより前に集約配置するのは、インライン展開によってエントリーブロックが分割された際に動的 alloca（ループ反復ごとにスタックが増大し続け、SROA 最適化も阻害される現象）化するのを防ぐためです。
スタック検査は、`llvm.frameaddress(0)`（プロローグ直後の副作用のないスタックポインタ値）を許容範囲 `[__stack_low + 4096, __stack_high]` と比較し、範囲を逸脱した場合は即座に `llvm.trap` を実行します（`llvm.stacksave` は副作用属性を持つため LICM やベクトル化を阻害しますが、`frameaddress` は安全に最適化を維持できます）。
この検査はトラップ情報の計測処理よりも前に挿入されるため、`@tz.stack.check` もコンテキスト ID を受け取ることができ、スタック溢れを `stack overflow` として正確に診断報告します。計測処理が後から付与するレポーター関数や `tsuzuri_trap_site` 自身はスタック検査を持たないため、スタックポインタが不正な状態のままでも安全に呼び出すことが可能です。同じ理由で、整数オーバーフローの報告（A23）の読み出し `tsuzuri_overflow_count`・`tsuzuri_overflow_hit` は検査の挿入前からあっても名前で除外され（`UNCHECKED_STACK_EXPORTS`）、トラップ後に Node のランナーが呼んでも再びトラップしません。
threads 環境では、WASM グローバル変数 `tsuzuri_stack_base` および `tsuzuri_stack_top`（インスタンスごとに設定、`!invariant.load`）が 0 であればメインスレッド、非ゼロであればワーカースレッドのスタック範囲として判定します。4096 バイトの安全マージンは、スタック検査を持たない `task-wasm-threads.c` および `wasm.ll` のフレームサイズの合計（`-O0` で 640 バイト）を十分に包含する設計となっています。
threads 有効時はインポートセクションの最大メモリサイズも同一の値となり、`--emit object` ではヒープ定数のみを保持します。ビルドキャッシュキーにはオプション設定と置換後の IR が含まれるため、上限値の変更も正確に反映されます。
検証は、`tests/wasm_memory.mjs` による `-O0`／`-O3` でのデフォルト出力不変性、2 GiB 超過時の room 形式と境界動作、スタック検査、マニフェスト設定、診断メッセージの検査、`tests/wasm_threads.mjs` によるワーカーのスタック溢れが隣接ブロックを破壊しないことの確認、ならびに `tests/wasm64.mjs`（Node.js 24）による wasm64 の上限・境界動作・4 GiB 超のホストバッファ・テストスイートの実行確認によって行われています。

### ホスト境界

Tsuzuri の関数内部に外部の C/C++ 関数を直接インポートして実行することはできません。
コンソール出力用の `putchar` は、コンパイラが自動生成したエントリーラッパー関数のみが保持します。数値演算が外部の `printf` に依存することはありません。
公開 ABI は、従来のスカラー型に加え、共有借用の i64／f64／ubyte 配列、両文字列型（`string`、`utf8string`）の入力、所有権付きバッファの戻り値、およびスカラー型のみで構成されるレコード型を安全に取り扱うことができます。
`abi.rs` が型、データの受け渡し方向、およびレコードのフィールド名を統一的に分類し、`llvm_abi.rs` が C ヘッダーとラッパー関数を生成します。
バッファ入力はポインタと長さから直接記述子を構築し、共有配列は SSA 値として、文字列はスタック記述子への参照として渡されます。
`string` は 16-bit の UTF-16 コード単位、`utf8string` は 8-bit の UTF-8 バイト列です。UTF-8 のバリデーションにはデコード失敗を値として返す既存の内部デコーダを用い、入力データを余分に確保・変換することはありません。
レコード型は専用の ABI 構造体と内部表現の間で再帰的に相互変換され、`bool` や狭幅整数は 32-bit に正規化されます。関数の戻り値は出力先ポインタ（out pointer）に書き込まれ、バッファの所有権のみがホスト側へと安全に引き渡されます。
メモリアロケータは、拡張 ABI の使用時に限り弱いシンボル（weak）として `tsuzuri_alloc` および `tsuzuri_free` を公開し、ネイティブではシステムの `malloc`／`free`、WASM では既存の WASM ヒープを使用します。
POSIX ネイティブ環境のアロケータは、共通のフックテーブルから境界ランタイムの存在を検知し、メモリの確保と解放を安全な追跡経路へとルーティングします。これは拡張 ABI を使用するデフォルトネイティブ IR における内部変更ですが、公開シグネチャそのものには一切影響しません。WASM および Windows におけるデフォルトアロケータは従来の動作を維持します。
### allocator の選択（F13）

`Alloc.__counter` は `i64 -> i64` の std 専用組み込みで、`@tz.heap.counter` の読み取りへ下がります（F14）。runtime symbols の収集でこの名前を見つけた場合、Host/Small は `E2000`、System かつ TestRunner 以外は `E1018`、TestRunner は Counting を自動選択します。明示の Counting も受け、counter だけで確保しないプログラムにも heap を連結します。弱いホスト ABI を付ける条件は明示の allocator のままなので、自動計数で WASM import や不要な公開 ABI は増えません。
`heap-counting.ll` は `[6 x i64]`（allocs, reallocs, frees, requested bytes, live, peak）を持ち、出来事・要求量・現在量・最大量を原子的に更新します。`@tz.heap.counter(index)` は 0〜5 を読み、それ以外をトラップします（資源失敗ではない不正な添字のトラップです）。公開 ABI `tsuzuri_alloc_stats` は 0・2・4・5 を従来どおり 32 バイトへ書きます。

heap runtime は `emit_program` の末尾で一つだけ連結し、`@tz.alloc`／`@tz.free`／`@tz.realloc` を `define internal` で定義します（呼び出し側は allocator を知りません）。`llvm::Allocator::System` は従来の `heap-native.ll`・`heap-wasm.ll`・`heap-wasm64.ll`（threads は lock 版）、`Host` は `heap-host.ll`（WASM では `tsuzuri_heap` からの import）、`Counting` は対象の heap を `counted_base` で `@tz.alloc.base` などへ改名し、その上に `heap-counting.ll` を置きます。`Small` は native だけで、`heap-small.ll` と `heap-small-native.ll` を置きます。WASM・`--trap-mode return`・`--freestanding`・カーネルの出力との組み合わせは `E2000` で、ライブラリ API（`emit_program`）も WASM には `E2000` を返します。
`Small`（PM05）は native の 64-bit だけです。`heap-small.c`（freestanding な C11）を `generate_heap.py` が target 中立の `heap-small.ll` にし、手書きの `heap-small-native.ll` が `@tz.alloc`・`@tz.free`・`@tz.realloc` を `heap-native.ll` と同じ契約（null でトラップ、新しいサイズ 0 は解放、null のブロックは新規確保）で `tz_small_*` の上に定義します。`heap-small.ll` は属性グループもメタデータも `nsw`・`nuw` も持たないので、`numeric.ll`・`math.ll`・デバッグ情報と番号が衝突せず、チェック付き算術の IR の検査（`bounds_checks`）にも触れません。入口の 3 関数は `internal` です。`tz_small_alloc` と `tz_small_free` は `noinline` で、`noalias`・`allockind`・`alloc-family` を持ち、LLVM は `malloc`・`free` と同じ扱いで、読まれないまま解放される確保を取り除き、戻り値を別のポインターと別名にならないものとして扱います。取り込むと、呼び出し側に fast path が並んで確保が呼び出しに見えなくなり、この最適化を失うので（record_sort は 2 倍の時間がかかりました）、呼び出しのままにします。`tz_small_realloc` は不透明です。外から見える記号は、全体で共有する状態へのポインター `@tz_small_shared`（弱い hidden。実体は各オブジェクトの `internal` な BSS で、リンカーが残した 1 つを全オブジェクトが使います）と、テスト専用の `tz_small_stats`（弱い hidden）だけです。スレッドごとのキャッシュは `internal thread_local` の 1 つで、`task.c` のスレッドにも C のホストのスレッドにも同じ形で付きます。
0〜512 バイトの確保は 16 バイト刻みの 32 区分で、ブロックにヘッダーはありません。区分は 1 MiB の region（`malloc(2 MiB)` から 1 MiB 境界に切る）ごとに 1 つで、アドレスから 2 段の表（`top[a >> 34]` の葉に region ごとの 1 バイト）で引きます。表に無いアドレス（512 バイトを超える確保、`task.c` などの C のコードが確保したバッファ）は `free` へ渡します。スレッドのキャッシュは区分ごとの後入れ先出しのリストで、空なら中央のスタックから 4096 バイト分の鎖を 1 本取り（無ければ region の未使用部分から鎖を切り）、鎖 2 本分を超えたら直近に解放した 1 本分を中央へ返します。中央のスタックは区分ごとの try-lock で守り、取れなければ待たずに、確保は C ライブラリの `malloc`、解放は手元のキャッシュに残して次の解放でやり直します。待つ処理も、`pthread_*`・`mmap` も使いません。IR が宣言する外部関数は `malloc`・`realloc`・`free` と LLVM の intrinsic だけです。
Windows の `lld-link` は同じ記号の弱い定義を持つ 2 つのオブジェクトを COMDAT なしでは結合できず（`numeric.ll` の弱い関数も同じです）、Windows では Tsuzuri のオブジェクトを 2 つ以上リンクできません。そのため複数オブジェクトで 1 つの状態を共有する検査は Windows では行いません。
`Host` と `Counting` は各ブロックの先頭 16 バイトに要求サイズを書き、ホストへ渡すサイズと統計を解放の経路によらず一致させます。トラップの理由は関数名で分類する（`traps::runtime_kind`）ので、確保の失敗の `@llvm.trap` は `@tz.alloc`・`@tz.realloc` とその `.base`（threads では `@tz.heap.alloc.unlocked`・`@tz.heap.realloc.unlocked`）の本体にだけ置きます。`@tz.heap.counter` の不正な添字は確保の失敗とは異なるトラップです。
`--freestanding` は `--allocator host` に加えて CPU ディスパッチを使わない経路（`emit_native_build` を通らない）で出力し、IR が C ライブラリを要する runtime（IO・OS・タスク・引数・`write`）を宣言したら `E2000` にします。`--emit header` の出力には IR がないので、同じ build の object が持つ library の IR を別に生成して検査します。
128-bit 値、ソフトウェア浮動小数点型、任意の所有入力、借用参照の戻り値、およびクロージャ環境の直接的な ABI 公開はサポートされていません。
外部シンボルのインポートはユーザーが記述した `extern` 宣言からのみ発生し、リンク名、ハンドル型、コールバックを使用しないプログラムにおいては、生成される IR、C ヘッダー、および WASM インポートの構造に変化はありません。
生成バインディング（E13）と `--emit shared` は IR・WASM・C ヘッダーを変えず、既存の export（`memory`、`tsuzuri_alloc`、`tsuzuri_free`、`tz_*`、`tsuzuri_trap_site`、`__indirect_function_table`）だけを使います。グルーはモジュールが宣言した import だけを渡して import を足さず、threads・IO・Debug・WASI・host allocator のモジュールは明示的な例外で拒否します。record の offset は `record_layout` と同じ規則で求め、`export def` の `ref H` 引数は、非拡張の wrapper でもハンドルを slot へ置いてから借用として渡します。
外部ライブラリのリンク入力はネイティブ実行ファイルのビルドでのみ有効です。`wasm-ld` の `--export-table` はコールバックラッパーが存在する場合にのみ渡され、変数を捕捉した関数値が ABI を越えて直接渡されることはありません。
GUI、ユーザー入力イベント、ネットワーク通信、非同期 I/O、およびイベントループは、ホスト環境との境界で適切に取り扱われます。ファイル操作、環境変数、システム時刻、乱数生成、および子プロセス起動は標準ライブラリの OS API が安全に仲介し、ユーザー定義の `extern` や公開 ABI を無秩序に増やすことはありません。

### 出力保護

コンパイラは入力全体の型検査が正常に完了した後、出力先と同一のファイルシステム上に作成した専用の一時ディレクトリ内でビルド処理を行います。
すべての関連ツールの実行が成功した後にのみ、アトミックなリネーム（rename）によって最終成果物を公開配置します。
この出力保護機構はルートパッケージのファイルだけでなく、プロジェクトに読み込まれたすべてのソースファイルに対して適用され、Unix 系 OS ではソースファイルへのハードリンクの作成も厳しく拒否されます。
他のプラットフォームにおいても、アトミックな置換処理によって別のハードリンク先の内容が意図せず書き換えられる事故を防ぎます。
出力ファイルを事前に `truncate`（切り詰め）して空にすることは決して行いません。

## 開発と検証

### 開発リポジトリの索引（G32）

`scripts/repo-kb.mjs` は Node 20 以降と git だけで、チケット・決定・責務表・診断の発行箇所・テスト・文書を
`target/repo-kb/` に索引化します。Rust 側の Facts／KB とは別の開発用ツールです。
入力ハッシュで鮮度を確認し、照会の前に古い索引を作り直します。出力の隣のロックで構築・鮮度検査・読み取りを直列化し、
全索引と manifest が同じ入力の世代になるように公開します。ロック自身と出力は入力ハッシュに含めません。
`brief <ID>` は指定した Phase の文脈を
既定 24,000 コードポイント以内に収め、削った部分の取得コマンドを残します。
見出し・Rust の文字列とテスト領域は反復走査し、出力に実行日時やホスト固有の絶対パスを加えません。
`check-guide-lite.mjs` は参照の存在を検査するだけで、文書を書き換えません。

```sh
node tests/repo_kb.mjs
node scripts/check-guide-lite.mjs
node scripts/repo-kb.mjs build
```

### コンパイルの段階別時間

`src/timings.rs` は `TSUZURI_TIME_PASSES=1` のときだけ時計と段の記録を使い、CLI の終了時に stderr の最後へ
`{"time_passes":1,"phases":[{"name":"load","ms":0.412}],"total_ms":7.702}` の形式で 1 行を出します。
値は ms、小数 3 桁です。同じ段は最初に現れた位置に合算し、入れ子にはしません。
`Phase` の RAII で診断による早期 return でも段を閉じ、`Report` が成功・失敗・LSP 終了のどの経路でも出力します。
`Report::start()` は前の報告の段を捨てるので、コマンドを続けて実行するプロセス（`watch`）は 1 回ごとに報告を出し、
そのプロセスの `main` の報告は `disarm()` で出しません。
並列に実行する段（PB04）も `Phase` は呼び出しスレッドだけが開閉し、ワーカーは `timings` に触れないので、
段は wall-clock で、合計は total を超えません。

- `load`: CLI のプロジェクト読み込み。`parse`: ソースの構文解析と frontend cache からの復号。
- `check`: 展開、名前・型の検査、抽象の所有権と Copy の推論。`specialize`・`closures`・`ownership`: 単相化、
  閉包の lowering、具体化後の所有権検査。
- `emit`: LLVM IR、header、GPU カーネルの生成と target ごとの IR 変換。Windows ABI・CodeView・no-red-zone、
  WASM の heap／host／runtime の処理と freestanding header の IR の検査までを含み、外部ツールの前に閉じる。
  `cache`: frontend cache の準備・保存と whole-build cache の鍵・読み込み・復元・保存・掃除。
- `tool.<実行ファイルの stem>`: `run_tool` が待ち合わせる Clang・リンカーなど。stem の空白・日本語も保持し、
  性能レコードで同じ名前を使います。cache の内部での版の照会は `cache` に含みます。
- `split`・`units`（PB07）: codegen unit に分けるビルドだけの段です。`split` は IR の分割、`units` は unit の鍵の計算・unit cache の読み書き・
  ファイルの書き出しで、並列の worker が走る間も呼び出しスレッドだけが開閉します。unit のコンパイルの待ちは `tool.<clang の stem>` です。
- 引数解析、生成後の GPU 検査、成果物の書き出し、`run` の子プロセスは `total_ms` にだけ含みます。

成果物、診断、終了コードは変えません。`1` 以外は無効で、無効時は時計・記録用の文字列・mutex を使いません。
比較時だけ、Mach-O の既存の UUID／署名と、native Windows Clang の標準 COFF object の
`TimeDateStamp`（offset 4 の 4 bytes）を除きます。残りの byte とサイズを検査し、生成物や compiler flags は書き換えません。
PX01 の記録と PX03 のビルド比較は、この同じ JSON を使います（[性能指標の記録](benchmarks.md#性能指標の記録)）。

### 検証コマンド

コンパイラの健全性を検証するコマンドと、変更の種類ごとに回す範囲は、[README の「検証と性能測定」](../README.md#検証と性能測定)にまとめています。ここには、各テストが何を確かめるかを書きます。
`node tests/ai_cost.mjs target/release/tsuzuri` は、コンパイラを変更せず AI 作業コストの定義・独立 golden・37 タスク・診断ループ・外部実測・能力に応じた静的サイズと KB/MCP を検査します（[AI 作業コスト](ai-cost.md)）。

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
連結リストのテストでは `tests/lists.rs` と `tests/fixtures/lists` を用い、配列と同様の検証項目に加え、区切り記号と演算子の共存、型クラスの適用、配列との混在、可変参照による不変性の迂回防止、chunk の型と `List.cons` の容量の定数、一括で作るリストの確保が 1 回であることを検証します。`tests/fixtures/list_chunks` の E2E suite は、`List.cons` で伸ばしたリストの chunk の境界（要素 1 個の最初の chunk から 4,093 slot の上限まで。16 bytes の slot と、大きさのない要素も）とその前後の長さで、走査・添字・`List.tail`・比較・複製・変換・パターン・表示と hash・スタックのリテラルを native と WASM の `-O0`・`-O3` で検証し、native では 100 万要素の clone・比較・`List.tail` も行います。`tests/primitives.mjs` は、chunk の確保数（100,000 要素のリストの確保が 1 回）、複製および所有要素・クロージャ環境の解放、100,000 回の末尾再帰、ならびに WASM のヒープ再利用と、100 万要素のリストが既定の 16 MiB のヒープに収まることを検証します。
記憶域のテストでは `tests/storage.rs` が `new` 構文のパース、束縛変数や一時値リテラルのエントリーブロック `alloca` 配置、`new` やメモリ移動・`&mut` に伴うヒープ確保の位置、64 KiB 上限の境界判定、および末尾ループのエントリー配置を LLVM IR レベルで検査します。`tests/fixtures/storage` は `tests/primitives.mjs` からネイティブおよび WASM の `-O0`／`-O3` で実行され、ネイティブでは各呼び出しにおけるヒープ確保回数（スタックに束縛されたリテラルは 0 回、`new` やスコープ外への移動・Copy による複製は各 1 回以上）とメモリリーク 0 を厳密に照合します。

数値ランタイムを変更する際は、`src/runtime/numeric.c` を編集した後に `python3 src/runtime/generate.py` を実行して `numeric.ll` を再生成します。
続けて `python3 src/runtime/generate_math.py` を実行して `math.ll` も再生成し、メタデータおよび attribute ID が `numeric.ll` の最大値より後方へ割り振られるようにします。
`Math` モジュールを使用する場合にのみ `numeric.ll` の後に `math.ll` が結合されます。`numeric.ll`・`math.ll`・生成コードが重複して宣言する LLVM intrinsic は、`math.ll` の有無によらず、`emit_program` の最後にシンボル名ごとに最初の宣言だけが残ります（`math.ll` の結合直後の `declare` の整理は従来のまま残しています）。
トラップおよびループ用の生成メタデータ ID は、両ランタイムの最大 ID より後方の範囲に予約されます。
math 生成スクリプトは、必要な musl 1.2.5 のソースコードを個別にコンパイルしたうえで `llvm-link` を用いて結合します。
検証済みのビルド環境は Apple Clang 21 および llvm-link 21 であり、環境変数 `TSUZURI_CLANG` および `TSUZURI_LLVM_LINK` によってツールパスを指定可能です。
保存処理に先立って同一の Clang で IR のコンパイル検証を行い、未解決の外部関数参照、FMA 命令の暗黙生成、fast-math フラグ、および意図しない拡張精度の混入を厳格に拒否します。
通常の Cargo ビルドにおいて Clang や llvm-link への依存関係を追加することはありません。アップストリームのライセンス条項およびソース取得元のハッシュ情報は `runtime/musl` に同梱されています。

浮動小数点数の平方根（sqrt）および丸め処理は、f32／f64 では型付き intrinsic および明示命令を用い、その他の広幅形式では既存の整数数値ランタイムを使用します。f16 の平方根は、経路が `Hardware` のときだけ `llvm.sqrt.f16` です（丸め処理は常にソフトウェア）。
明示的な `Math.fma` のみが、AArch64 ネイティブ環境（ビルド機が AArch64 のときの `--target native`、または AArch64 の triple）の f32／f64 において `llvm.fma` 命令へと lowering されます。ハードウェア命令を保証できないネイティブターゲット、WASM、およびその他の浮動小数点形式では、高精度なソフトウェア実装 `tz_soft_fma` が使用されます。
通常の乗算や加算に対して `contract` や fast-math 属性を付与することはなく、libm の `fma` インポートを無用に生成することもありません。
ソフトウェア FMA は、係数の積と加数を符号付き多倍長整数として正確に合成し、既存の `pack` 処理によって 1 回だけ丸め込みます。指数差が $3p + 8$ 桁を超える極小の非ゼロ項については、丸めの中点（tie）の判定方向のみを保存する sticky digit へと縮約します。
これにより、固定長の `LIMBS` 領域を無駄に拡大することなく、f128 や decimal128 の極端な指数差を持つ演算も正確に取り扱うことができます。特殊値（NaN、無限大など）は事前に処理され、decimal 型は十進表記のまま厳密に計算されます。
`Array` の `pairwise`、`Neumaier`、`dot`、`dot_fma` は通常の標準ライブラリ関数として実装されています。`pairwise` は 1 回の所有コピーを `Array.__set` でインプレース更新し、各段の隣接ペア木構造を維持します。従来の `sum` や通常の内積計算は左結合の順序と個別の丸め処理を維持します。
ソフトウェア平方根 `soft sqrt` は目標の量子指数を算出した後、整数二乗比較によって仮数部を探索し、2 つの丸め候補の中点の二乗と比較して 1 回だけ正確に丸めます（decimal から binary への変換は行いません）。最小値・最大値関数（min/max）は NaN の伝播と符号付きゼロを厳格に扱い、クランプ関数（clamp）は引数の順序関係の検査を維持します。
初等超越関数（Elementary）は f32／f64 のみに提供されます。musl の標準的な f64 `atan2` 実装において 1 ulp（Unit in the Last Place）を超える誤差が確認されたため、有限の通常比の範囲については double-double 精度による範囲縮小と 31 次の多項式近似級数へと置き換えられました。
縮小後の変数値は 1/8 以下に収められ、多項式係数は 256-bit 精度から上位・下位に分割して保持されます（特殊値や極端な比率の処理はアップストリームの堅牢なロジックを維持しています）。
速度向上の誇大な主張は行いません。`tests/math.mjs` では、FMA を含む `BigInt` 参照実装による基本演算 759,675 件、および 256-bit `mpmath` 参照による初等超越関数 26,376 件をネイティブおよび WASM の `-O0`／`-O3` で照合検証しています。
f16 の単項基本演算は全 65,536 パターン、広幅形式は各 1,000 件の乱数パターンを含み、非 NaN におけるビット完全一致および実行時ヒープ非確保が確認されています。

f16 を LLVM の `half` で計算する経路（D10）は、`F16Path`（`Soft`、`Conversions`、`Hardware`）が `EmitOptions`・`Instrumentation`・`TestRunnerOptions` から `Globals` へ渡り、`FunctionEmitter::binary`・`cast`・`math_builtin` の 3 か所が、経路が許すときだけ `src/llvm_math.rs` の `half_binary`・`half_cast`・`half_sqrt` を呼びます。f16 の値の保存、ABI、デバッグ情報は経路によらず `i16` のままで、演算の前後でだけ `bitcast` で `half` と行き来します。`F16Path` の既定は `Soft` で、WASM は `emit_program` が常に `Soft` にします。`f16` を使わないプログラムの IR は、導入前とバイト単位で同一です。
経路は `driver::f16_path` が選ぶ純関数です。入力は、WASM か、ターゲットの OS とアーキテクチャ、`--cpu`、`CpuFeatures`（ホストの `f16c`・`avx512fp16`・`fp16`。`--cpu native` のときだけ見る）で、`std::env::consts` やホストの `cfg!` を関数の中で読みません。クロスコンパイルを持ち込む変更は、ターゲットの OS とアーキテクチャを渡し、ホストの機能は渡さない（`CpuFeatures::default()`）ようにします。AArch64 は、FEAT_FP16 の有無によらず `Hardware` です（定数 `AARCH64_HALF_WITHOUT_FP16`。`false` にすると、macOS と、`--cpu native` で `fp16` を検出したホストだけです）。x86-64 は `--cpu native` のときだけ、`avx512fp16` で `Hardware`、`f16c` で `Conversions` で、`--cpu generic` は `Soft` です。`Conversions` は、`f16`→`f32`／`f64` と `f32`→`f16` だけを `half` の `fpext`／`fptrunc` にし、演算と `f64`→`f16` は `Soft` のままです。F16C に `f64`→`f16` の命令がなく、LLVM が `__truncdfhf2` を呼ぶことと、`f32` を経由すると二重丸めになることが理由です。能力がなければ `Soft` に戻ります。利用者が経路を指定する手段はなく、`--cpu native` が使えないアーキテクチャの `E2000` は従来どおりです。経路は性能の選択で、結果のビットは変わりません。
`--cpu native` の x86-64 は、`clang -x ir` に `-march=native` と、選んだ経路の `-mf16c`・`-mavx512fp16` を渡し、コンパイラの検出と clang の検出が食い違っても `half` がライブラリ関数（`__extendhfsf2`・`__truncsfhf2`・`__truncdfhf2`）に下がらないようにします。`@cpu` の版は `target-features` がビルドの特徴を置き換えるので、x86-64 の level 1〜3 の版に `+f16c`（`Conversions`）か `+avx512fp16,+f16c`（`Hardware`）を足します（`src/llvm_cpu.rs`）。
`half` の命令は `fadd`・`fsub`・`fmul`・`fdiv`・`fcmp`・`llvm.sqrt.f16`・`fpext`・`fptrunc` で、fast-math フラグも融合もありません。`llvm.sqrt.f16` の宣言は intrinsic の集合で 1 度だけ出ます（`numeric.ll` と `math.ll` は宣言しません）。FEAT_FP16 のない AArch64 では、LLVM が演算ごとに `f32` へ広げて 1 回の `f32` 演算をし、`half` へ丸め直します。`+`・`-`・`*`・`/`・平方根は、`f32` の仮数 24 bit が `2 × 11 + 2` 以上なので二重丸めにならず、結果は 1 回丸めと同じです。生成した機械語がライブラリ関数を呼ばないことは、`tests/f16_hardware.rs` が AArch64（Linux の gnu と musl、Windows の msvc、macOS の各 `-O0`・`-O3`）と x86-64（`-mf16c`、`-mavx512fp16`）で確かめ、フラグなしの x86-64 が呼ぶことも負の対照として確かめます。
結果が NaN のときは、ハードウェアの符号とペイロードに頼らず、`numeric.c` と同じビット（quiet NaN `0x7E00` に、`+`・`-` は左辺、`*`・`/` は両辺の符号の排他的論理和）へ整数の `select` で置き換えます。NaN の判定は `(bits & 0x7FFF) > 0x7C00` で、`fcmp uno half` は使いません。`Math.sqrt` は NaN の入力をそのまま返し、変換は入力の符号を保った目的の型の quiet NaN を返します。この置き換えを外した試作は macOS arm64 で C の `_Float16` と同じ時間でした。その費用と、ほかの形の試作は [benchmarks.md](benchmarks.md#f16-のハードウェア経路d10) にあります（1 回ずつの測定）。
`tests/f16_hardware.rs` は、IR の形（既定は `Soft`、WASM は経路によらず同一で `half` と import がない、`f16` を使わない IR は不変、決定性、演算・比較・平方根・変換）、経路ごとの機械語、`@cpu` の版の特徴、`driver::build` の経路を検査します。`tests/f16_hardware.mjs` は、全 65,536 パターンの単項、二重丸めの罠を含む境界の値、中点、乱数の 2 項演算・比較・変換を、JavaScript の倍精度で厳密に求めた参照（コンパイラを使わない）と、NaN のビットまで照合します。native の `half` の IR（`-O0`・`-O3`、macOS arm64 では FEAT_FP16 なしの `-mcpu=generic` も。`-O3` でベクトル化される一括ループも）、WASM のソフトウェア経路（import なし）、`--cpu generic` と `--cpu native` のオブジェクトを調べます。実行した経路は、オブジェクトの `tz_soft_` の有無から分類して表示し、環境変数 `TSUZURI_F16_EXPECT`（テスト専用）で期待する経路を決めると、別の経路を通ったときに失敗します。`tsuzuri test` も `-O0`・`-O3`・wasm32 で確かめます。この実行が確かめるのは実行したホストの経路だけです。CI の `targets.yml` は、macOS arm64・Linux x86-64・Linux arm64 の runner で `TSUZURI_F16_EXPECT` を付けて実行します。Linux x86-64 は runner の機能に応じ `native=conversions|hardware` を許容し、現時点では `conversions` を参照と照合済みです。AVX512-FP16 の `hardware` と Windows の実行は未確認です。`benchmarks/run-f16.mjs` は、ソフトウェア経路、`half` 命令、C の `_Float16` を同条件で測ります。
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
`tests/task_runtime.c` はハードウェア機能検出および pthread 呼び出しを計測用フックに差し替え、条件変数による実際の並行実行、共有プールの上限維持、全スレッドの完全ジョイン、逐次フォールバック、ならびにスレッド生成・ジョイン失敗時の適切な診断報告を検証します。同じフックで、257 添字のグループが取る mutex が `2 + (CPU 数 - 1)` 回以下であること、signal と broadcast の数、眠っているプールが必要な数だけ起こされること（取りこぼせば止まる形の検査）も確かめます。さらにフックで 2 つの順序を決定的に確かめます。グループを公開した時点で最初の添字は投入したスレッドが取り済みであり、ワーカーが残りを全部走らせるまで投入側を止めても、最初の添字を走らせるのは投入側であること。グループを最後に終えるワーカーが、添字の release・投入側の起床・自分を空きと数えることを 1 つの critical section で行い、投入側が戻った後に実行中と数えられたままの形（release の後に mutex を放す・取る、release を lock の外で行う、空きと数えてから release する）が検出されることです。
`tests/task_stress.c` は、複数の host から入れ子のグループ、`parallel_results` の最小の失敗添字、65,536 添字の 1 グループ、チャンネルの pipeline を同時に走らせ、結果を実行の様子ではなく計画だけから決まる閉じた式と照合します。時間を合否にせず、起床の取りこぼしは alarm で失敗にします。単一の host の pipeline に deadlock の判定が出ないことの assert は、数千の group に 1 件ほどしか出ない誤判定を偶然に頼って見つけるもので、決定的な検査は `tests/task_runtime.c` が受け持ちます。`tests/tasks.mjs` が `-O0`／`-O3` と、`TSUZURI_TSAN=1`・`TSUZURI_ASAN=1` のときの sanitizer 付きで実行します。
単なる経過時間の短縮をテストの合否判定基準とすることはせず、通常の関数呼び出しが完了した後にバックグラウンドで不要なワーカースレッドが残存しないことを厳格に検査します。

Async は `tests/async.rs` が opaque / non-Copy、実際の借用を持つ callback 環境の拒否、非 Copy の結果、決定的な IR と到達可能性、CLI・ABI・生成宣言を検証します。抽象的な引数の placeholder loan だけを呼び出し側の具体的な環境へ延期し、std 本体の検査を免除しません。`tests/features.mjs` の `async` suite は仮想時計、順序、短絡、飽和、深い継続、`For` を native/WASM の `-O0` / `-O3` で確認します。`tests/async.mjs` は host executor、native の登録・退役・他スレッドからの完了・`Task.parallel`、wasm32/wasm64 の JSPI、生成グルーの直列化・入力スナップショット・失敗保持・世代、native の test/debug-test/bench を検証します。native の各呼び出し後に mailbox を含む `live == 0` を確認し、ASan 時には生成 IR の関数にも `sanitize_address` を付けます。

コンピュテーション式は、`tests/computations.rs` が拡張子ごとの宣言制限、複数型クラスの適用、ビルダーの各構文、単相化、クロージャ捕捉、生存期間、未実装操作の検出、ネスト展開深度、および未使用ビルダーの型検査を検証します。
`tests/computations.mjs` は、ネイティブおよび WASM の `-O0`／`-O3` においてカスタム短絡評価、複数回の yield、入れ子の反復、`Delay`／`Run` の有無による挙動差異、評価順序、数値境界、トラップ、およびタスクとの相互合成をテストします。
ネイティブ環境では全関数呼び出し後の未解放メモリバイト数が 0 であることを検証し、WASM 環境ではデフォルトの 16 MiB を超える累積確保の反復耐性を検査します。また、型注釈のない配列 bind と手書きのループ処理においてメモリ確保量が完全に一致することを確認します。
`.tt` および `.tc` におけるエラー診断の正確な位置特定、ファイル出力保護、ならびに決定論的な IR および WASM 生成も確認されます。
`tests/call_specialization.rs` と `tests/fixtures/computations/Optimization.tz` は、静的に既知の関数値、動的関数値、スコープ外へエスケープする関数値、可変引数の評価順序、所有権を持つ捕捉値および戻り値、相互再帰、ならびに特殊化予算上限に達した際の安全な通常経路へのフォールバックを網羅的に検査します。
同じファイルの `standard_let_chain_workers_are_direct_and_allocation_free` は、`let!` が 2 つの標準 Maybe／Result の連鎖から到達する worker が確保・閉包の複製・間接呼び出し・apply wrapper を持たないこと（native と wasm32、IR の決定性を含む）を、`known_local_function_values_call_directly` は、let で束縛した関数値（捕捉付き lambda・捕捉なし lambda・部分適用）の呼び出しが直接であることを固定します。
`exhausted_worker_budget_completes_chains_instead_of_cutting_all_of_them` は、600 個の連鎖が予算を使い切るとき、worker の定義が 1,024 個のまま、先頭の連鎖から到達する worker がすべて直接呼び出しで、最後の連鎖が通常の経路を使うことを、`workers_within_the_budget_are_emitted_in_request_order` は、予算内の program の worker が連続した id で要求順に出力されることを固定します。
`union_results_return_through_one_slot` は戻り値 slot を使う関数（constructor・引数を返す exit、`facts::apply` が本体の前に scope の宣言を足す参照引数を含む）と使わない関数を、`tail_forwarding_to_a_cycle_keeps_every_ret` は、相互再帰・局所を経由する再帰・引数の閉包を経由する再帰を含む、呼び出しを返しうる exit が平文の `ret` のままであること、`no_call_result_leaves_through_a_slot_in_real_programs` は、ベンチマークと fixture の IR で呼び出しの結果が slot に入らないことを固定します。`tests/computations.mjs` は、この 5 つの再帰の 1e8 段が native `-O2`・`-O3` で通ることを実行して確かめます。
コンピュテーション式の実行テストには、520 個の異なる継続によって予算を超過する大規模ケースや、300 個の `let!` の連鎖（約 2,400 個の worker を要求）で予算を使い切って結果が変わらないことの確認、`-O3` 最適化下で 262,144 回のコールバックを実行するストレステストが含まれ、ネイティブおよび WASM の双方でメモリ解放、スタック消費量、および演算結果の正確性が確認されています。
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
`tests/local_recursion.rs` はローカルな `let rec` の構文と診断、捕捉の分類、リフトした関数の直接呼び出しとループ（入れ子のグループ、ラムダ式・継続の中のグループ、代入された捕捉）、グループ、`match` の読み取り、コンピュテーション式、未使用の警告を検証し、深さ 1,000,000 の末尾再帰を `-O0` のネイティブで実行します。`tests/features.mjs` の suite `local_recursion`（`tests/fixtures/local_recursion`）は、ネイティブと WASM の両方で、閉じた式から求めた期待値と、リフトした関数のループを検査します。`tests/local_recursion_fuzz.mjs` は、乱数で作った入れ子のグループ・クロージャ・代入される捕捉・借用した文字列と配列を持つプログラムを、独立した JS の評価器（期待値は仕様から求め、コンパイラの出力は使わない）と照合する差分テストです。リフトを変えたときに手で回します。

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

標準 OS API、ハッシュコンテナ、および文字列補間は実装済みですが、Windows ネイティブの OS API 対応（現在は `E2002` エラー。[G10](../_features/_completed/G10-windows.md) の後続の作業）、WASI preview2 および WebAssembly コンポーネントモデルへの対応（E13 の対象外で、計画チケットはありません）、ネットワークソケット API（E09）、ハッシュコンテナにおける SIMD を活用した群探査アルゴリズム（F08）、ならびに書式指定における Unicode 書記素クラスタ（grapheme cluster）幅の考慮（D09）は今後の実装課題として計画されています。
