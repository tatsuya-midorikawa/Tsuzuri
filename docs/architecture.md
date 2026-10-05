# コンパイラ構成

実装言語は Rust。binary リテラルの正確な丸めには `rustc_apfloat` を使い、
LLVM の C API／Rust バインディングには結合しません。
テキスト形式の LLVM IR と、バージョン差の小さい Clang／LLD の CLI を境界に使います。
`cargo build` に LLVM 開発ヘッダーや CMake は不要です。

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
| `src/diagnostic.rs` | ソース ID とファイル内位置、診断の順序・重複除去・表示／収集上限、human／JSON lines |
| `src/syntax.rs` | トークン、構文木、構文資源上限 |
| `src/lexer.rs` | UTF-8 を壊さない字句走査、コメント、数値 |
| `src/docgen.rs` | 宣言ASTからの公開API Markdown、型・制約・regionの描画、決定的ページ順 |
| `src/parser.rs` | Pratt parser、宣言と式、トップレベルのエントリーコード、深さの制限 |
| `src/parse_control.rs` | インデント本体、for／while／match、関数ガード、ラムダ式、パターンと認識器名 |
| `src/check.rs` | 全モジュールのシグネチャ収集、名前解決、型付き IR、レイアウト、公開 ABI |
| `src/control.rs` / `recursion.rs` | 型付きループ、短絡するパターン手順と束縛、認識器呼び出し、参照グラフの再帰検査 |
| `src/exhaustiveness.rs` | 型付きの被覆パターン、usefulness による match の網羅性・到達不能な節の検査、不足する値の例（check の子モジュール） |
| `src/computation.rs` | ソース種別の検査、`.tc` ビルダーの収集、型検査前の関数・継続への展開（check の子モジュール） |
| `src/polymorph.rs` | 型変数の単一化、型クラス・インスタンス、モジュール関数制約の解決、制約の伝播、単相化（check の子モジュール） |
| `src/closures.rs` | 匿名関数の検査、自由変数の捕捉、lambda lifting、公開ABIの完全適用ラッパー |
| `src/numeric.rs` | プリミティブ名、整数・浮動小数点接尾辞、binary／decimal リテラルの丸めとエンコーディング |
| `src/constants.rs` | 定数の依存順評価、型別演算と資源上限、既存リテラルへの展開、定数・一時値の借用引数（`temporary_borrows`） |
| `src/exceptions.rs` / `src/llvm_exception.rs` / `std/Exception.tz` | `try ... with ... finally`・`@checked`・単項 `+` の型検査（check の子モジュール）と lowering、`**` の lowering、std の `Exception`／`ExceptionKind` と `Err` の instance |
| `std/BigInt.tz` | `bigint`。符号と、下位から並べた 10^9 進の桁（`[i64]`）による std ソースだけの多倍長整数（不透明 record）。コンパイラは型名 `bigint` を `BigInt` に、`I` 接尾辞のリテラルを桁の配列を渡す `BigInt.make` の呼び出しに写すだけ |
| `src/ownership.rs` | 部分 move、借用の競合、最後の使用、分岐の合流、参照の寿命 |
| `src/ownership_control.rs` | 反復の固定点、ガードの読み取り専用別名、分岐・認識器の一時値の寿命 |
| `src/llvm.rs` | SSA、phi、末尾ループ、所有値の解放、借用先、ホスト・ラッパー、C ヘッダー |
| `src/llvm_debug.rs` | 共通採番によるDWARFメタデータ、型・変数・関数と式のソース位置 |
| `src/llvm_imports.rs` | externのABI wrapper、リンク名とWASM import属性、コールバック引数、所有結果の受領検査 |
| `std/IO.tc` / `src/llvm_io.rs` / `src/runtime/io.c` | 不透明なIOモナド、入口での実行、標準ストリーム、WASMホスト境界 |
| `std/Os.tz` / `File.tz` / `Dir.tz` / `Path.tz` / `Env.tz` / `Time.tz` / `Random.tz` / `Process.tz` / `src/runtime/os.c` / `src/runtime/os-wasi.c` | OS API。純粋な std ソース、`Os.__*` builtin（`src/llvm_io.rs` の `os_builtin`）、POSIX の runtime、`--wasm-host wasi` の WASI preview1 runtime |
| `std/HashMap.tz` / `std/HashSet.tz` | ハッシュコンテナ。コンパイラに型・builtin・runtime を足さない std ソースだけの実装 |
| `std/Format.tz` / `src/runtime/format.ll` | 文字列補間の書式指定。`Format.parse`／`Format.pad` と、padding の runtime 補助（結合は `src/llvm_display.rs`） |
| `src/simd.rs` / `src/llvm_simd.rs` | 128-bit vector/mask型、lane型族、境界検査とLLVM vector lowering |
| `src/llvm_control.rs` | 直接の反復・switch・定数表、パターン手順の分岐と全経路の解放 |
| `src/llvm_bulk.rs` | 配列連結・リストの一括走査・安定 merge sort の型付き builtin lowering |
| `src/llvm_compare.rs` | 配列・リスト・タプルの借用構造比較、短絡と段階的メソッド適用 |
| `src/derive.rs` / `src/llvm_hash.rs` / `src/llvm_display.rs` | 導出instanceのAST合成、canonical Hash、引用・コレクション表示、文字列補間（`Interpolated`）の一括結合 |
| `src/llvm_math.rs` / `src/runtime/math.c` / `src/runtime/musl/` | Float基本数学と型別Elementary関数、固定版の移植可能な数学実装 |
| `src/recursive.rs` / `src/llvm_recursive.rs` / `src/runtime/recursive.ll` | 具体型ごとの再帰成分、所有ノード、追加確保なしの解放と反復複製 |
| `src/llvm_frame.rs` | `new` なしのリテラルのフレーム領域、実行時のアドレス判定、スコープ外への移動時のヒープ移送、フレームを考慮した解放 |
| `src/call_specialization.rs` | 非 escaping な関数引数の固定点解析、既知の継続・読み取り専用捕捉の判定、LLVM worker の特殊化予算 |
| `src/ranges.rs` | 型付き IR 上の配列添字の範囲証明（規則 R1–R5）と関数ごとの `RangeFacts`。証明できた添字だけ境界検査の分岐を省く |
| `src/runtime/numeric.c` / `numeric.ll` | 多倍長整数による f16／f128／decimal 演算、比較、広幅／形式間の変換、最短往復表示・解析、書式指定付きの数値表示（`tz_soft_format_spec`） |
| `src/runtime/string.ll` / `utf8string.ll` / `heap-*.ll` | UTF-16／UTF-8 バッファ操作・明示的な符号化変換、ネイティブ確保、WASM の再利用・結合可能なヒープ |
| `src/runtime/closure.ll` | 関数値の環境の複製と解放。環境ごとの処理は LLVM emitter が生成 |
| `src/runtime/cpu.c` | native標準i64配列和、CPUID/OSXSAVE/XCR0、atomicなvariant cache |
| `src/runtime/heap-*.ll` の `tz.realloc` | native realloc と、WASM の隣接空き領域再利用・確保コピー fallback |
| `src/runtime/task.c` / `task-wasm.ll` / `task-wasm-threads.c` | native pool、WASM既定逐次、opt-in共有メモリWorker pool |
| `src/runtime/wasm.ll` | 128-bit 乗除算・剰余・シフトの freestanding 補助 |
| `src/stdlib.rs` / `std/` | 埋め込みの標準ライブラリのソース、予約 std モジュール名、std の仮想パス |
| `src/driver.rs` | ソースファイルの列挙、`Main.tz` 選択、LLVM／LLD 起動、ステージング、出力保護。ツールは `TSUZURI_*` → 配布物（実行ファイルの 2 段上に `manifest.json`）の `bin/` → `PATH` の順に解決（`resolve_tool`。cache key も同じ解決を使う） |
| `src/main.rs` | CLI オプションと診断・警告の表示、`toolchain info` |
| `src/copies.rs` | 具体化後の暗黙の複製の一覧（`copies::sites`）、`--warn implicit-copy` の `W1006`、inlay hint の元になる配列・リストの複製（`costly_sites`） |
| `src/lsp.rs` / `src/semantic.rs` | stdio言語サーバー、Unicode位置変換、単相化前の型・定義位置インデックス。定義・参照・ローカルの有効範囲・record 型の式を索引し、型付き木が落とすフィールド・record・case 名は checker の `name_uses` から集める。rename と quick fix は編集後の再解析で診断と名前の結び付きの不変を確かめる。入力中の補完・signature help・semantic tokens・複製の inlay hint は直前の成功索引を共通の接頭辞・接尾辞で写して使う |

doc commentはlexerのDocComment tokenとして保持し、parserで宣言のDocumentation(text, span)へ添付します。
def/fn結合では署名側から引き継ぎ、誤配置はE0002です。生成するchecked functionにはdocsを複製せず、型/ownership/LLVMの意味は変えません。
docコマンドは通常のproject検査後にソースASTから署名を描画します。再parseは文書の構文情報だけに使い、独立した型推論やLLVM生成は行いません。
private/instance実装を除き、.tz/.tcを共有するstd moduleは一ページに結合します。constの初期化式は省略し、builtin一覧は言語仕様を参照します。
driverの専用公開処理はmarker・ソース包含・symlinkを検査し、一時ディレクトリから全体をrenameします。旧出力の退避と失敗時復元はbuildのファイル出力処理とは独立です。
formatterのfingerprintはdoc本文を比較しspanだけを消去します。SemanticIndexは宣言位置ごとに本文を一回保持し、hoverは既存のtarget spanから説明を取得します。

## 性能設計の原則

**最速・最強を目指すことは、コンパイラ・ランタイム・組み込み関数・今後の標準ライブラリに
共通する設計要件です。** 高水準で安全な API の内側では、対象機で利用できる CPU 命令、
SIMD、複数 CPU コア、GPU、最適化済みカーネルを活用する設計にします。
ただし、利用率やバックエンドの名前ではなく、正しい結果までの総時間を最小化します。
LLVM に渡すだけで高速と判断せず、生成コードと実測で経路を確認してください。

現在の実装と将来の要件は区別します。

| 項目 | 現在の実装／今後の要件 |
|---|---|
| スカラー CPU | i8〜i64／i8u〜i64u と f32／f64 の変換は LLVM の直接命令・飽和 intrinsic。`as` と変換 builtin の意味・性能経路を揃える |
| SIMD | `-O3` のループ／SLP 自動ベクトル化。連続配列・型の特殊化・不要コピーの除去で最適化可能な IR を生成する。`--cpu native` はビルド機の命令セットを有効化 |
| 移植性 | 既定の`--cpu generic`はターゲットbaseline。同梱i64配列和だけ実行時ISA選択。`native`は配布条件にビルド機ISAを含める |
| 複数 CPU コア | `Task.parallel` の遅延起動する常駐プール。CPU 数で追加スレッド数を制限し、呼び出し元も自分のグループを進行する。WASM は逐次 fallback。自動並列化は未実装 |
| GPU | 実験的kernel抽出・CPU参照・strict整数WGSLとWebGPU host試作。通常のTsuzuri runtimeへの実GPU自動接続、float GPU、自動offloadは未実装 |
| WASM | bulk-memory対応。SIMD128、threads、`--wasm-host wasi` は独立した明示opt-in。既定は非SIMD・importなし（IOは`tsuzuri_io`）・逐次で、OS APIはE2000で拒否 |

新しい builtin／標準ライブラリでは、要素ごとの汎用関数呼び出しだけを基本実装にせず、
型・連続性・サイズが分かる一括操作を設計してください。正確な基準実装を持ち、
スカラー／SIMD／並列 CPU／GPU で同じ契約を検証します。既存の所有権・借用・不変性から
別名関係や独立性を証明できる場合だけ最適化し、根拠のない `noalias` 等を付けません。

自動バックエンド選択では、確保、コンパイル／初期化、ディスパッチ、転送、同期、
結果の回収まで含めた閾値を対象ハードウェアで測定します。小さい処理を常に GPU に送ったり、
スレッドを増やせば速いと仮定したりしないでください。GPU 常駐データは転送の繰り返しを避けます。
未対応機では契約を保つ CPU 経路を選べるようにし、選択経路を観測可能にします。
GPU 等を明示要求した場合の利用不可・実行失敗は診断し、黙って成功扱いや結果変更をしません。

高速化で整数の折り返し、飽和、最近接・偶数丸め、NaN、符号付きゼロ、評価順序、トラップを
変更しません。浮動小数点の集計順変更や FMA 融合が必要なら、まず別 API／明示モードの契約を
設計し、既存の演算へ暗黙適用しません。追加した経路には境界値・参照実装との照合と、
同条件の C/C++ 比較を用意します。共有 CI は正しさと経路の退行を検査し、
性能の閾値判定は安定した専用環境で行います。

WASM featureはBuildOptions.wasm_simd/wasm_threadsで表します。driverはSIMD有効時-msimd128、既定-mno-simd128を渡します。
LLVM IR出力はfeature要件をコメントへ記録します。tests/wasm_simd.mjsはllvm-objdumpの命令解析とBigInt参照で検証し、即値の0xfdをSIMD opcodeと誤認しません。

threadsはWASM/object出力専用です。C11のfreestanding task-wasm-threads.cを-matomics/-mbulk-memoryで生成し、wasm-ldのshared/import-memoryで結合します。
heap-wasm.llの同一allocatorを内部名へ変更し、heap-wasm-threads.llのlock wrapperから呼ぶため、reallocの内部alloc/freeも一回のlock内です。
共有状態はlinear memoryに置き、wasm-ldの一回限りのdata初期化を使用します。各instanceの__stack_pointerとstackの範囲（tsuzuri_stack_base・tsuzuri_stack_top、mainは0）だけをhostが設定します。
groupはcaller stackに置き、queue lock内でatomicに仕事を取得してからcallbackを実行します。remaining公開後にgroupを参照せず、callerはlock内でunlinkして戻ります。
callerも仕事を進め、入れ子は自groupを優先して他groupも手伝います。epochのwait/notifyでidle待機し、heap lock内で利用者callbackを呼びません。
Nodeホストsrc/runtime/wasm-threads.mjsが明示worker数を初期化し、初回groupでspawn_workersを呼びます。各workerの256KiB stackをheapから確保し、closeは完了後にWorkerを終了します。
memory省略時はbytesのimport sectionからenv.memoryのmaxを読み、initial/maximumともそのpage数でshared memoryを作ります（既定256page、WebAssembly.Moduleを渡した場合も256page）。
Worker trap/初期化失敗は共有failedとlock poison bitを公開して全waitを解除します。以後の実行は拒否し、trap後の解放は保証しません。通常の言語Resultとは別です。
tests/wasm_threads.mjsはO0/O3のstack sentinel、workerのstack溢れのトラップ、atomic barrier、heap残量、入れ子、bulk、失敗、object、SIMD/debug併用と決定性を検証します。

src/runtime/trap-boundary.mjsは単一スレッドのWASM向けの同梱JSホストで、コンパイラは参照せず、生成物も変えません。createBoundary(module, { imports, sites })のcallがexport呼び出し1回を境界にします。
WebAssembly.RuntimeErrorは{ reason: "trap", site }とside tableの位置、V8のRangeError（SpiderMonkeyはInternalError）によるstack枯渇は{ reason: "stack" }で返します。ホストimportの例外は同じobjectのまま再送出します。
トラップは巻き戻さないため、例外の出たinstanceは分類のためtsuzuri_trap_siteを一度呼ぶ以外は二度と使わず、次のcallで同じmoduleから作り直します。threadsのmoduleは拒否し、単位は既存のcreateThreadPoolのpoolです。
nativeではdriver::probable_stack_exhaustionがtsuzuri run／testの子プロセスの終了signal（SIGSEGV、SIGBUS）から推定してE2005／テスト失敗の理由を付けます。tests/trap_boundary.mjsがO0/O3の17 caseを検証します。

native objectの境界（E14 Phase 2）は--trap-mode returnで有効にし、native（object、llvm、header）だけで受け付けて--trap-infoを含みます。各export tz_nameにint32_t tsuzuri_try_name(tsuzuri_trap_info *trap, 結果, 引数...)（status 0成功、1トラップ、2入れ子）を足し、header（typedefはTSUZURI_TRAP_INFO_DEFINEDで重複を避けます）とIRのthunkを出します。
setjmpはsrc/runtime/trap.cの中だけにあり、IRに持ち込みません。llvm_traps::instrumentがtz.trap.reportの先頭へtsuzuri_trap_raise(site, kind)を足し、境界のthread-local frameがあればlongjmpで戻り、なければ従来どおり報告してllvm.trapで終わります。
heap-native.llのmalloc、realloc、freeはtsuzuri_tracked_*へ置き換えます。返すpointerは通常のmallocと同じで、追跡情報は境界ごとの別の双方向listと拡張するhash表に置き、解放するpointerを表で探します。追跡情報の確保が失敗したらpointerを解放してNULLを返し、reallocの失敗では元の領域を保ちます。トラップなら全blockを一括でfree（drop、lockの解放はしません。Tsuzuriのコードは巻き戻らず、runtimeはcallbackの前にlockを手放します）、成功なら追跡情報だけを捨て、ホストが所有する結果を残します。
境界本体（lock、list、hash表）はtsuzuri_boundary_runの自動変数にせず、追跡対象外のmallocで確保してvolatileなpointerで持ちます。setjmpの後に変更された非volatileの自動変数は、longjmpで戻った後は不定で、最適化（-O2以上で実測）で解放処理が古い値を読むためです。確保に失敗したらabortします（メモリ不足は従来どおり失敗します）。
POSIX nativeの通常object・task.c・trap.cはweakなtsuzuri_trap_hooks（owner、item、resume、allocate、releaseの5つの関数pointer）を共有します。trap.cのconstructorが表を設定し、境界runtimeがなければ全slotはNULLです。公開tsuzuri_alloc/freeもこの表を使うので、通常objectと境界付きobjectのどちらを先にリンクしても確保と解放が一致し、ホストのexternが返す所有bufferも境界へ登録できます。未定義の弱い関数への依存はありません（macOSのlinkerはこれを拒否するため）。
task.cは表のownerがあればgroupを投入したthreadの境界を取得し、worker（と投入したthread）のitemを表のitem（tsuzuri_boundary_item）の中で実行します。itemのトラップはgroupへ最小indexで記録し、以後のitemを始めず、全itemの終了後に投入したthreadが表のresume（tsuzuri_boundary_resume）で境界へ戻ります。トラップしたtaskの状態は未定義なので、先にErrを返したitemがあってもトラップを返します（B06のdropがその状態を読むため）。
C runtime（task.c、cpu.c、io.c）は動的確保を持たず、確保の経路は@tz.alloc／@tz.realloc／@tz.freeだけです。os.cも結果の所有バッファだけを`tsuzuri_alloc`で渡しますが、パス名・列挙・出力の取得のための一時領域とファイル表にはlibcのmalloc／realloc／freeを直接使い（ファイル表は開いているファイルがある間だけ存在し、ほかは呼び出しの中で解放します）、境界runtimeの追跡経路を通りません。Windows COFFの埋め込みruntimeはE2002、extern callback（E12）との併用はE2000です。
tests/trap_boundary_runtime.c（C。ASan・UBSan・TSan、並列度1〜32）、tests/trap_return.mjs（native object・IR、O0/O3、確保のlive == 0）、src/main.rsとsrc/driver.rsの単体テストが検証します。

native実行ファイルのスタック枯渇（E14 Phase 3）はsrc/runtime/stack.cが報告します。constructorがmain threadにsigaltstackとSIGSEGV／SIGBUSのhandlerを置き、task.cのworkerは-DTZ_STACK_GUARDでtsuzuri_stack_thread()を呼んで自分のstackを登録します。
faultアドレスが登録したstackの下端の窓にあればstderrへtrap: stack overflowを書いてabort()し、窓の外なら既定の動作へ戻して再送します（同時に溢れた別threadがあっても報告は失われません）。macOSはpthread_get_stackaddr_np、Linuxのmain threadはgetrlimit(RLIMIT_STACK)とAT_EXECFNの末尾（muslのpthread_getattr_npはmain threadで現在のmapping幅しか返さないため）、workerはpthread_getattr_npで窓を決めます。
runtimeはnative実行ファイルでだけ、llvm::has_recursionが関数の直接呼び出しに閉路を見つけたプログラムにだけ足します（再帰しなければ溢れず、ビルドに20〜60msを足さないためです）。-gでなければ同じclang呼び出しの-x cで、-gならDWARFを持たない別objectでコンパイルします。
driver::runはstderrのtrap: stack overflowを見てE2005にstack overflowと書き、見つからなければprobable_stack_exhaustionの推定へ戻ります。tsuzuri testの実行ファイルはstderrを捨てて子プロセスの終了signalだけを見るので、この報告を持ちません。
tests/stack_overflow.mjs（macOS、O0/O3。main thread、worker、-g、再帰しないプログラムと目的のobjectにhandlerがないこと、スタック外のfault）、tests/trap_locations.rsのrun_reports_stack_overflow、llvm.rsのfinds_the_programs_whose_stack_can_overflowが検証します。

nativeのCPU dispatchは同梱Arrayソースを確認したemit_native_buildでだけ有効にします。対象は単相化したArray.sumのref [i64] -> i64です。
通常のLLVM API/--emit llvmは従来の独立IRを維持し、driverのexe/objectはtsuzuri_cpu_sum_i64出現時だけcpu.cをtask runtimeと同じC連結経路へ追加します。
C11のatomic関数ポインターをacquire load/acq-rel cmpxchgで一度選択します。feature bit0=SSE4.2、bit1=AVX2で、AVX2はOSXSAVE/AVX/XCR0のXMM+YMM状態を要求します。
baseline/SSE4.2/AVX2はuint64 wrapping sumで同一結果。ISA属性はCのtarget属性からClangが生成し、GNU ifuncやcompiler-rtのCPU modelには依存しません。
AArch64と未知環境はbaseline、SVE/SVE2・float dispatch・任意ユーザー関数の多重化は未実装です。公開runtime入口はweak/hiddenです。

externはProgram.externsにsignatureを持ち、通常関数の型付きHostCall wrapperへ下げます。外部ABIは型検査で具体型へ確定し、通常の関数値/部分適用/所有権/特殊化を共有します。
HostCallは副作用ありとしてchildren/may_mutate/ownershipへ登録し、extern wrapper自体は到達性のrootから外します。
宣言はBTreeSetで一度生成し、WASM属性でmodule/nameを固定します。E05のrecord正規化・buffer型・pointer/UTF検査とallocatorを再利用します。
所有buffer結果はout descriptorのlenを-1で初期化し、未設定・負数・overflow・不正範囲を受領時に拒否してから通常のdropへ渡します。

externはlink名（`extern "symbol" def`、`extern "module" "symbol" def`）を持てます。HostImportのexplicitがtrueなら、symbolをそのままnative名とWASM nameにし、moduleを`wasm_module`（既定`tsuzuri`）にします。指定がないexternの名前・IR・header・WASM importは変わりません。
symbolは255byte以下のC識別子で、`tz_`・`tsuzuri`・`__`で始まる名前とRESERVED_HOST_SYMBOLS（生成IRとnumeric runtimeが自分で宣言する名前。abi.rsのテストが宣言との差を検出）を拒否します。同じsymbolの複数宣言は、関数型とWASM moduleが一致すれば一つのdeclareにまとまります。明示symbolの宣言はホストのheaderが持つので、生成headerにprototypeを出しません。

`extern type Name`はType::Handle（修飾名）で、Copyでもcloneでもなくdrop glueを持たない葉の型です。自動で閉じるにはDropを持つrecordで包みます。LLVMではptr（wasm32ではi32）で、`ref H`はslotから読んだhandle自体をホストへ渡します。
ABIに置けるのはextern・export・コールバックの引数と結果だけで、record fieldとbuffer要素には置けません。Type::exportableは変えず、abi.rsのabi_scalarで区別します。headerは`typedef struct tz_handle_<長さ付きの修飾名>_s *`をprototypeより前に一度だけ出します。

リンク入力は`--link`・`-l`・`-L`と根packageのmanifestの`[native]`で、nativeの実行ファイルだけが対象です。clangへは既存の引数の後に、manifestの後にCLIの順で`-L`・path・`-l`を足します。
build cacheのキーはリンク入力の内容を知らないので、入力がある間はcacheを使いません。出力pathが入力と同じならE2003で、他のtarget・出力・check/fmt/docでの指定はE2000です。

コールバックは、関数型の引数を持つextern（callback extern）を直接・全引数で呼び、その位置にトップレベルの利用者関数の名前だけを渡す場合に限ります。check.rsのvalidate_callbacksが型付け後に全関数を辿って検査し、他の形はE1008です。
呼び出し側が`host_call`を直接出し、callback externの関数本体は出さず`%tz.closure`も作りません。関数型の実引数は`ptr @tz.callback.<修飾名>`で、`llvm_abi::wrapper`がinternalなC ABI wrapperを関数ごとに一つ、id順にexport wrapperの後で生成します。
wasm32ではwrapperのアドレスが関数tableの添字になるので、IRに`@tz.callback.`があるときだけwasm-ldへ`--export-table`を渡します。callbackのないプログラムの出力は変わりません。

## 不変条件

**Whole-build cache:** cache.rsはSHA-256のstreaming実装とNISTベクトルを持ち、既存serde_jsonでmetadataを扱います。parse/check/IRのcacheは作りません。
compiler executable全体のhashを使い、同じgit commitにある未コミット開発版も区別します。build.rsのgit文字列だけには依存しません。
キーは長さ付きfieldでversion/host/options/action、全sourceとmanifestのpath/bytes/origin、生成IR、compiler/tool binary digest、tool --version、関連環境変数を含みます。
cpu nativeはClangのtarget macro群も含めます。pathはUnicode正規化せずOS表現を保ち、絶対source mapとpackage identityも区別します。
artifact/traps/dwarfを通常ステージへ復元し、実行権限とsidecarを保持してpublish_outputsへ渡します。hit/missともsource/manifest/hardlink保護を迂回しません。
cache rootは専用markerが必要で、symlinkを拒否します。各fileのsizeとSHAを検証し、metadata欠落・破損・未知formatはmissです。trust boundaryは同一OSユーザーのprivate cacheです。
保存はキーごとの非待機create-new lockで重複writerを避け、全file完成後にdirectory renameします。lock競合は保存を省き、ビルドを待たせません。I/O失敗は警告で元ビルドを維持します。
GCは4096entryまで走査し、last-used/2GiB/30日を使って最大128件だけ回収します。古い部分entry・lock・一時領域も対象で、上限はsoftです。
macOS debug exeのDWARFは出力名を持つため、その場合だけoutput pathもキーへ含めます。普段の別出力先へのartifact再利用は維持します。
tests/cache.mjsが実CLIのhit（tool起動数）、miss、破損、同時writer、no-cache、実行権限、trap/DWARF、依存変更を検証します。

**Windows MSVC:** native_compile_argsはコンパイラのCPUに合わせてx86_64-pc-windows-msvcまたはaarch64-pc-windows-msvcを選び、POSIXフラグと分離します。Win32 task adapterは既存schedulerへSRWLOCK/CONDITION_VARIABLE/INIT_ONCE/CreateThread/WaitForSingleObject/CloseHandleを提供します。
windows_abiは型検査済みexport一覧にだけdllexportを付け、writeをCRT _writeの32-bit count/resultから安全に拡張します。UTF-8 bytes保持のため出力fdをbinary modeにし、コードページは変えません。
Windowsにも既存128-bit helperを同梱し、MSVC CRTだけでlinkできます。CPU dispatchはWindowsではportable baselineです。
Rustのfile identityはWindows限定same-fileのsafe APIを使い、unsafe禁止を維持します。fs::renameの既存出力置換とhardlink保護はWindows専用テストで検証します。
COFFのruntime同梱object結合はE2002です。exeを優先し、LLVM+runtimeを一度だけ明示linkする経路を残します。
Windows CIとtests/windows.mjsを追加。macOSで実Windows SDKによるC/IRのO0/O3 COFF/PE link、全Rust targetのWindows cfgを確認済みですが、Windows runnerでの実行ゲートは未確認です。

**結果付きTask:** Task.parallel_resultsは通常builtin schemeとcold task closureを経由し、専用TypedExprKindをllvm_task.rsへ下げます。
runtime ABIはi64 tsuzuri_task_parallel_results(i32 (*run)(context,index),context,length)で、戻り値-1が成功、他は最小失敗indexです。
native/threadsの既存groupにrun_result/failureを加え、queue lock内で未配布lengthを短縮してremainingから未開始分を引きます。開始済み分のremainingが0になるまで戻りません。
LLVMは一時Result配列とi8 started配列を確保します。callbackは環境を消費して結果を書いた後にstartedを公開し、join後はそのflagで初期化済み結果と未開始closureを区別します。
成功payloadを出力へ、選択errorだけを返却へ移動し、他を既存dropで解放します。一時Result配列にはaggregate dropを行わずraw bufferだけfreeするため、移動済みslotを再dropしません。
recursive Resultの外側nodeもpayload移動後にfreeします。trap-infoは既存callback ABIを保ち、task codeへだけcontextを伝播します。
tests/tasks.mjsとwasm_threads.mjsは41結果・4trap、所有配列/closure/再帰payload、順序、最小error、冷たい破棄を検証します。C schedulerはASan/UBSan/TSanでも検証済みです。

**HKT:** ClassDecl.kindは明示kindを持ち、TypeExprKind::ApplyのIdent headが`'f`なら型変数適用です。既存の名前付きhead/formatter経路を維持します。
Classesのconstructor arityと署名/制約からのkind環境を使い、値型位置は飽和を要求します。初版のkind引数はすべてTypeで、higher-order kind/型別名は拒否します。
Type::Partialはconstructor宣言と末尾固定引数、Applicationはheadと適用引数を持ちます。Type全体は四wordを維持し、map_type/substitute/Inference::resolveで飽和時に既存Record/Union/Array/List/Vec/Taskへ正規化します。
HKT methodはclass receiverと固有の値型変数をfreshにし、具体instanceの完全signatureからmethod特殊化引数を決定します。default methodも既存の内部関数と共有Specializerを使います。
instance headとmethodで同名の変数を使う場合は、生成関数のmethod変数をalpha分離して名前捕捉を防ぎます。
値の型にPartial/Applicationが残った場合はLLVM入口の検査でE1015です。名前付きgeneric宣言の通常Type::Variableは従来どおり残せます。
tests/higher_kinds.rsとfeatures.mjs higher_kindsがOption/Result・default・generic関数・ローカル注釈・全constructor形状・owned heapを検査します。stdへFunctorを自動導入しません。

**GPU Phase 1:** gpu.rsは通常の型付き・単相化済みIRを検査し、既知呼び出しの有界graphを抽出します。独立した型推論・数値評価器は作りません。
GpuKernel.cpu_referenceは既存LLVM emitterを再利用します。Gpu.init/mapのcallback制限はclosure lowering後に検査し、first-class APIによる回避を拒否します。
std/Gpu.tzは明示CpuReferenceだけを構築します。opaque Device/Bufferのnon-Copy性はTypeと記号的所有権検査で共通に扱い、格納fieldへのアクセスは既存opaque gateを使います。
WGSLは各値を一時letへ順序付きで評価し、if/短絡の分岐内でだけ対応する式を評価します。同幅整数castはbitcast、符号付き算術はu32のwrapを経由します。
WGSLの型・float/trap契約からi64/f64/strict float/div/remはshaderで拒否します。CPU参照での許可とshader生成の許可を混同しません。
CLIの--emit wgslは単一exportのkernel projectを通常の出力保護経路で公開します。WebGPU host試作はdevice limits・shader診断・所有buffer・error scope・queue同期を扱い、明示要求をCPUに縮退させません。
LLVMのenum scalar aliasは前方参照できないため、全enum aliasをrecord/union構造体定義より先に生成します。構造体の相互前方参照は従来どおりです。
検証はcargo test --test gpuとtests/gpu.mjs。TSUZURI_WEBGPU=1では実adapter上のshader・init/map・resident chain・境界も実行します。

**モジュール:** 1 ファイルに 1 モジュールを強制し、名前はファイル名から取得します。
root配下を再帰探索し、`SourceFile.relative_path`を正規化した順で処理します。`Geometry/Point.tz`の内部名（key）は`Geometry.Point`で、ソースでは`Geometry::Point`と書きます。
file入力のrootは親、directory入力はそのディレクトリです。hidden項目を無視し、source/directory symlinkは拒否します。
source4096・directory1024・module16要素/255byteを上限とし、標準ライブラリの予約は先頭の名前空間に適用します。
`.tz` はコード、`.tt` は複数の型クラス宣言、`.tc` は一つのビルダー実装です。
型クラスは `.tt` に限定し、インスタンスは `.tz`／`.tc` の通常の実装です。
拡張子を除いた名前が重複するファイルは拒否します。旧 `.tzr` は直接入力を拒否し、自動列挙の対象外です。
`analyze_modules` は `("Name.tz", source)` などの組を受け、ファイル種別も含めて検査します。
互換の拡張子なし `("Name", source)` と `analyze(source)` はファイル種別未指定のメモリ上 AST を検査し、
既存のフロントエンド利用者向けに混在宣言を許します。driver は必ず拡張子を渡し、この互換経路を使いません。
関数とレコードはモジュールで修飾した一意な名前を持ち、LLVM の内部シンボルにも修飾名を使います。
他モジュールの関数は修飾が必須で、レコード・union・case・クラスは自モジュール優先・
他モジュールで一意なら無修飾名を解決します（std との優先順位は後述）。型付き IR の関数・レコード参照はプロジェクト全体で一意な ID です。
公開 ABI は従来の `tz_name` を維持し、エクスポート名の衝突は型検査で拒否します。
`private` は型検査時の名前解決（`Names` の `NameInfo` による可視性判定）だけで完結し、
型付き IR・LLVM の内部シンボル・公開 ABI を変えません。他モジュールの private 候補は
無修飾レコード名の解決先にも曖昧性の候補にもなりません。public 宣言からの private 型の漏れは
解決済みの `Type` ではなく元の `TypeExpr` を走査して、漏れた型参照の位置で報告します。

**名前空間:** パーサーは先頭の文脈キーワード `namespace A::B` と、その後の `using A::B` を `Program.namespace`／`Program.usings` に読みます。
ソースの名前空間のパスは `::`、コンパイラ内部の名前空間・key・完全名は `.` で区切ります。変換は `module_identity`・`module_path`・`namespace_path`・`canonical`・manifest の境界だけで行い、診断と LSP の表示は `.` を `::` に戻します。
lexer の `mark_paths` は空白なしの `Ident::Ident` の連鎖のうち、最後の要素が英大文字で始まるものと行頭の `namespace`／`using` の後のものを `TokenKind::PathSep` にします。`def`／`rec`／`and` の宣言名の直後と `x::xs` は `DoubleColon` のままです。
パーサーは `PathSep` でつながる要素を `A::B::Mod` の一つの識別子に読み、メンバーは従来どおり `.` の連鎖です。
コンパイラが key で組み立てる修飾名（alias の展開、derive、単相化）は先頭に `::`（`check::KEY_PATH`）を付け、利用者が `.` で書いた名前空間のパスと区別します。
`Type::display` は record／union／extern type を key から作った名前で表示し（`check::type_display`）、モジュール名と同じ名前の型はモジュールの名前だけにします（`Geometry.Point.Point` は `Geometry::Point`、`Option.Option<i64>` は `Option<i64>`）。内部の名前・IR・LLVM の型名は `Geometry.Point.Point` のままで、型から構文を作り直す単相化（`polymorph::key_name`）も key の名前を使います。
`SourceFile.namespace` は root パッケージの既定名前空間（manifest の `namespace`、package 名の PascalCase、manifest がなければフォルダー名）で、依存と std は空です。
`lib::module_identity` が相対パス・宣言・既定名前空間からモジュールの key と名前空間を決めます。key は宣言のないファイルでは従来の相対パス名、既定名前空間の内側を宣言したファイルでは既定名前空間を除いた名前、それ以外は完全名です。
key は型検査・型付き IR・LLVM シンボルの修飾名なので、既定名前空間を宣言しても IR は変わりません。入口は key ではなく `ModuleInput.entry`（root の `Main.tz`）で選びます。
`check::Names` は完全名から key への表、key ごとの名前空間と `using` の解決結果を持ちます。`module_path` は `Name` か `A::B::Name` を受け、参照元の名前空間、`using`（モジュール名だけ）、外側の名前空間、グローバルの順に完全名を探し、モジュール名だけのときは最後に key として探します。`.` を含むパスはモジュールを指しません。
`canonical` は最後の `::` の後の最初の `.` までをモジュールとして key に置き換え、関数・case・型・クラス・active pattern・ビルダーの検索の前に一度だけ適用します（`KEY_PATH` 付きの内部の名前はその印を外すだけです）。
名前空間とモジュールを `.` でつないだパスは解決せず、`E1002`／`E1004` の診断が `namespace_spelling` で `::` の書き方を示します。
モジュールのパスは同名の record／union／型別名／extern type も表し、その型の完全名はモジュールの完全名です。`check_module_type` は `Sample::Point.Point` のようにモジュール名を重ねた型のパスを、`case_path` は `Sample::Shape.Shape.Rect` を `E1004` にします（`KEY_PATH` 付きの内部の名前は対象外）。診断と hover は `type_spelling`・`semantic::type_name` で同じ名前を示し、LSP のメンバー補完はその型を出しません。`using` の曖昧さは `check_path`／`check_module` が検索の入口で `E1004` にします。LSP は `ModuleNames` で同じ規則を再現し、補完（`::` の後は名前空間の子、`.` の後はメンバー）・シグネチャヘルプ・semantic token に使います。

**パッケージ:** 読み込み層は`Tsuzuri.toml`のlocal path依存も扱います。`package.rs`が限定文法を解析し、driverは明示スタックでgraphの循環・名前・上限を検査します。
`SourceFile.package`にcanonical rootとnameのPackageIdを保存し、依存namespaceをrelative_pathへ付けます。型検査のUser/Std分類やprivateの境界は変更しません。
manifestは`Project.manifests`に保持し、source_forでは通常sourceの後のIDを使用します。ビルド・文書出力保護は両集合を対象にします。
全sourceはlogical path順、stdは末尾です。依存rootを親の探索から除き、同一rootを一度だけ読みます。build script・ネットワーク実行はありません。

**標準ライブラリ:** `std/` のソースは `stdlib::SOURCES` として `include_str!` で埋め込み、
`analyze`／`analyze_modules`／`Project::load` のすべてで利用者のソースの後に追加します。
parse 順は入力順なので、利用者のソース ID と `Project.root` は std の有無で変わりません。
`analyze_modules_with_std` は Rust テスト向けに std を差し替えます（空スライスは std なし）。
各モジュールは `ModuleOrigin`（`User`／`Std`）を持ち、`stdlib::RESERVED_MODULES` の名前は利用者のモジュールに使えません。
std のパスは `std/Name.ext` の平坦な仮想パスで、出力保護の対象外です。userのrelative_pathと混同しません。
無修飾の型・case・レコード・クラスの解決は、自モジュール、完全修飾名（型ではモジュールのパスが表す同名の型も）の後、参照元が利用者なら
利用者のモジュール、std のモジュールの順に段階ごとに一意な候補を探し、参照元が std なら std だけを探します。
std の `export def` は拒否し、std の関数は利用者の関数と同じく修飾必須です。
std のソースは常に型検査し、`closures::lower` の後で到達可能性により間引きます。
`CheckedFunction.origin`（`FunctionOrigin`）が唯一の由来情報で、生成された `$lambda`／`$task`／`$builtin`／
`$case`／`$export` などの補助関数は持ち主の `module`／`test` を継承し、`parent` に持ち主の関数 ID を持ちます。
LLVM は利用者由来の関数をすべて出力し、std 由来の関数は利用者のテスト以外の関数・export・入口からの
参照で到達するものだけを出力します（`reachable_functions`）。名前付きレコード・union の型定義は、
利用者由来の型、出力する関数のシグネチャと本体から集めます。

`std/Option.tc`・`std/Result.tc` は通常のジェネリック union と関数だけで型・基本操作・ビルダーを定義します。
固有の型付き命令やランタイムはなく、union の tag 分岐・payload の move／clone／drop と既知継続の特殊化を共有します。
`get` は網羅的な match の失敗 case で `unreachable : unit -> 'a` を呼びます。
空の本体も成功として反復を続けるため、`Zero` は unit の成功です（Option は `Some ()`、Result は `Ok ()`）。
スカラー builder の到達可能な呼び出し経路に確保・間接呼び出しがないことを IR で検査します。
汎用 adapter の定義には確保が残り得るため、IR 全文の `@tz.alloc` の有無と実行経路の確保を混同しません。

**組み込み関数:** `Builtin` は `BuiltinScheme`（型変数、制約、`Std` の型、`UnsignedOf`／`WidenOf` の型族）で型を表し、
参照は具体化後の型引数を持つ `FunctionRef::Builtin(BuiltinInstance)` だけです。
修飾名の組み込み関数（`Task.run` など）はモジュール関数の後、型クラスのメソッドより前に解決します。
型族は引数が具体的な整数型に決まった呼び出し位置で解き、型変数のまま関数末尾に残れば `E1015` です。
組み込み関数はすべて scheme の引数個数を持つ `$builtin` ラッパーを経由して呼び、部分適用は通常の関数値と同じく下げます。
LLVM の定義は具体化ごとに一度だけ `@tz.builtin.name` に型引数の名前を `.` で連結したシンボルで出力し、
型名は `$A`（`->`）・`$L`／`$R`（`[`／`]`）・`$C`（`,`）で LLVM の識別子文字に置き換えます。

**エントリー:** `Main.tz` のトップレベルコードを合成した非公開関数、または `Main.main` の
関数 ID を保持します。両者の併用は拒否し、他モジュールや `Main.tc` の `main` は入口に選びません。
トップレベルの `let` は通常のローカル束縛へ下げ、モジュールの共有状態は導入しません。
ライブラリ出力にはコンソール・ラッパーやトップレベルコードの自動実行を追加しません。
トップレベルコードの値は、数値・`bool`・文字・文字列ならそのまま、ほかの型は `Display` の instance があれば `to_string` で表示し、なければ捨てます（`entry_result`）。
IO の `let!`／`do!` の後を通常の式で終えたトップレベルコード（`$implicit.root.$entry`）と、ビルダーを持たない既知の結果型（`def main :: i32` など）の本体は、
IO の bind を std の非公開 `IO.run` で順に直接実行します（direct style。直接実行できるビルダーは IO だけです）。

**標準入出力:** std/IO.tcはopaqueな`IO<'a>`に通常の`unit -> 'a` closureを保持します。pure/bind/map/Delay/Combine/For/While/MergeSourcesは通常ソースであり、別のタスク・GC・effect interpreterは導入しません。
IO.__read_line/__writeはstd由来のIOモジュールだけが参照できるbuiltinです。低水準readは`(i32 * [ubyte])`、writeはstatusを返し、Option/ResultとUTF変換はstdが処理します。
LLVMは既存のhost_result_slot/read_host_resultを再利用してdescriptorを初期化・検査します。IO専用の外部呼び出しに純粋性属性は付けず、所有バッファは同じallocator/dropを使います。
`IO<T>`の入口が`tsuzuri_main`を生成し、通常closure ABIの`unit, env, borrow=false`で一度消費実行します。結果TはFunctionEmitterの既存drop_valueで解放し、再帰型の解放登録も共有します（`IO<i32>`だけは解放せず終了コードとして返します。後述）。native executableはmainから呼び、object/WASMは明示ホスト呼び出しで、結果表示は付けません。
nativeのio.cは既存task/CPU runtimeと同じC結合経路へ必要時だけ追加します。fgetcのstdio bufferで行を読み、幾何増加bufferを通常allocatorで管理します。EINTRを再試行し、LF/CRLFを除きます。fwriteは部分書き込みを進め、flush失敗もstatusへ返します。
WASMはtsuzuri_ioの同期read_line/writeとmemory/allocatorを必要時だけ公開し、ホスト不在をno-opにしません。既定の計算専用モジュールにはimportを増やしません。
driverのrunはstdin/stdoutを継承し、stderrを読みながら転送します。JSON診断の場合だけstderrを保持し、異常終了診断へ含めます。
tests/io.mjsはnative/WASM O0/O3、cold/順序/EOF/符号化/失敗/ABI境界、対話CLI、object、ASan/UBSanと未解放byte0を検証します。

**OS API（E08）:** `File`／`Dir`／`Path`／`Env`／`Time`／`Random`／`Os`／`Process` は通常の std ソース（`std/*.tz`）で、OS に触れる操作はすべて遅延 `IO<Result<_, Os.Error>>` です（`Path` と `Random.Pcg` は純粋）。コンパイラが持つ境界は `Os.__read`／`__args`／`__write`／`__random`／`__clock`／`__sleep`／`__open`／`__handle`／`__close`／`__spawn` の10個の `Builtin` だけで、新しい型・クラス・runtime 関数は登録しません。
`IO.__read_line`／`__write` と同じく、std の `File`・`Dir`・`Env`・`Time`・`Random`・`Process`・`Os` 以外のモジュールからは参照できず、利用者のコードは E1022 です（`polymorph.rs`）。
第1引数の整数が操作を選びます。`__read` は 0 ファイル・1 ディレクトリ一覧・2 カレントディレクトリ（パスは無視）・3 環境変数・4／5 メタデータ（4 はシンボリックリンクを辿り、5 は辿りません）、`__write` は 0 作成して切り詰め・1 作成して追記・2 mkdir・3 rmdir・4 unlink、`__open` は 0 Read・1 Write・2 Append・3 CreateNew、`__handle` は 0 read・1 write・2 flush、`__clock` は 0 monotonic・1 Unix です（ナノ秒。失敗は `INT64_MIN`）。
状態値は、成功が 0、失敗が `(kind << 32) | (errno & 0xffffffff)` の `i64` です。kind は 1 NotFound・2 PermissionDenied・3 AlreadyExists・4 InvalidInput・5 InvalidEncoding・6 Interrupted・7 Other（`Os.ErrorKind` の宣言順）で、std の `Os.error_of_status` が `Os.Error { kind, code }` に戻します（Tsuzuri 自身が見つけた失敗の code は 0）。LLVM は `status == 0 || (status >> 32) - 1 < 7` を検査し、外れた値は `read_line` と同じく `BoundsCheck` でトラップします。`__open` はハンドル（正の値）か否定した状態値を返すため、負のときだけ符号を戻して検査し、`__clock` は検査しません。
バイト列を返す primitive（`__read`／`__args`／`__random`／`__handle`／`__spawn`）は、IO と同じ `host_result_slot`／`read_host_result` の descriptor を先頭の out pointer として受け、成功でも失敗でも書き込みます（失敗は NULL と長さ 0）。LLVM はその descriptor と状態値から `(i64 * [ubyte])` の組を作り、所有バッファは通常の allocator で drop します。`ref utf8string` は stack descriptor の pointer と長さへ、共有 `ref [ubyte]` は descriptor 値（pointer と長さ）へ展開して渡します。

**OS API のリンクと入口:** 到達した primitive だけを `declare i64 @tsuzuri_os_<name>(...)` として intrinsics の集合（`BTreeSet`）へ入れるため、宣言は重複せず出力は決定的で、OS API を使わないプログラムの IR と runtime は変わりません。`Os.__args`（`Env.args`）に到達したときだけ `declare void @tsuzuri_os_set_args(i32, ptr)` も宣言し、その宣言があるときに限って入口の wrapper を `@main(i32 %argc, ptr %argv)` にして、`tsuzuri_main` の前に argc／argv を保存します（`Env.args` は argv[0] を含みません）。ほかの入口は従来の `@main()` です。`@tsuzuri_os_` を含む IR にも、allocator は `tsuzuri_alloc`／`tsuzuri_free` を公開します。
native の `src/runtime/os.c` は、IR が `declare i64 @tsuzuri_os_` を含むときだけ driver が C runtime の単一 translation unit へ加えます。連結順は os.c、`trap.c`、task、`cpu.c`、`io.c` で、os.c が先頭なのは `_DARWIN_C_SOURCE`／`_GNU_SOURCE` を最初の `#include` より前に定義する必要があるためです。公開関数は weak／hidden です。
`IO<i32>` の入口は、結果を drop せず `tsuzuri_main` の戻り値、つまりプロセスの終了コードとして返します。`llvm::exit_code_entry` が std の `IO<i32>` かを判定し、ほかの `IO<T>` は従来どおり結果を drop して 0 を返します（以前は `IO<i32>` の値も捨てて 0 でした）。`Os.exit` はありません。`tsuzuri run` は、`exit_code_entry` の native プログラムが 0 以外で終了したとき、トラップとは別に E2005 `program exited with code N` を報告します（JSON 診断では stderr を添えます）。

**OS API と wasm・Windows:** wasm の既定出力は `tsuzuri_io` 以外のホスト import を持たないため、OS primitive に到達した wasm の object・LLVM IR・wasm 出力は、出力を書く前に E2000（`OS_WASM_MESSAGE`）で拒否します。`Path`・`Os` の純粋な補助関数・`Random.Pcg` は primitive に到達せず、import なしでビルドできます。Windows 上のコンパイラが native の exe／object を作るときは、OS primitive に到達していれば E2002（`OS_WINDOWS_MESSAGE`。`--emit llvm` は除く）です。G10 の Windows 実行は未検証のため、対応は主張しません。
`--wasm-host wasi`（`BuildOptions.wasm_host`。build だけの指定）は wasm32 の object・wasm にだけ許され、`--wasm-feature threads` とは併用できません（E2000）。LLVM IR とも併用できません（E2000）。os-wasi.c は object にコンパイルして結合するので、IR だけを出す経路では `tsuzuri_os_*`／`tsuzuri_io_*` が未解決のまま残るためです。`llvm::with_wasi_host` が標準 IO の `declare` から `tsuzuri_io` の import 属性を外し、`src/runtime/os-wasi.c`（freestanding C11）を別の object にコンパイルして結合します。os-wasi.c は os.c と io.c の関数と同じ `tsuzuri_os_*`／`tsuzuri_io_*` を `wasi_snapshot_preview1` の上に実装し、import は到達した関数の分だけ増えます。パスは preopen したディレクトリのうち `/` 境界で一致する最長の名前に解決し、どれにも一致しなければ最初の preopen からの相対です。`Os.Error.code` は WASI の errno、`Env.current_dir` は最初の preopen の名前、`Process.run` は `Other`（未対応）です。`_start` は `--emit wasm` で入口が IO のときだけ定義し（`-DTZ_WASI_START`）、`IO<i32>` の非 0 値は `proc_exit` へ渡します（`-DTZ_WASI_EXIT_CODE`）。object の `_start` は埋め込み側に任せます。WASI preview2 とコンポーネントモデルは未実装です。

**ファイルハンドルと子プロセス:** `File.Handle { id: i64 }` は runtime のファイル表を指す不透明な Copy 値で、`Drop` ではありません（std の型は `Drop` を実装できず（E1016）、`Drop` の値は `let!` の継続を越えられない（E1005）ためです）。handle は `(generation << 32) | (slot + 1)` で、open のたびに二度と使わない generation を割り当てます（`INT32_MAX` 回で `EOVERFLOW` の `Other`）。閉じた handle や古い handle は、同じ記述子番号を持つ別のファイルへ届かず `InvalidInput`（EBADF）になります。表は倍々に拡張し、開いているファイルがなくなると解放します。`File.with_open` は全経路で閉じます。ファイルは `O_CLOEXEC` で開き、ディレクトリは `InvalidInput`（EISDIR）で拒否します。
`Process.run` は `posix_spawnp` で shell を介さずに起動します。引数は NUL で区切って渡し、argv の1要素ずつになります。入力は子の標準入力へ書いて閉じ、標準出力と標準エラーは `poll` と非ブロッキング読み取りで同時に集めて、合計が 2^30 byte を超えると子を `SIGKILL` して `Other` にします。書き込み中の `SIGPIPE` は呼び出しの間だけ無視します。

**OS API の検証:** `tests/os_api.rs` は予約名・primitive の非公開（E1022）・型・宣言と runtime が到達したときだけ出ること・argv を受ける入口・`IO<i32>` の戻り値・`Random.Pcg` と `File.Handle` の不透明性を検査します。`tests/os.mjs` は native と WASI（`tests/os-wasi-host.mjs` が `node:wasi` で実行）の両方を `-O0`／`-O3` で、実ファイルシステム・環境変数・引数・終了コードと E2000／E2005 の診断に対して実行し、Node の `fs`／`os` の期待値と照合します。native と WASI の結果は system の errno を除いて一致します。追跡 harness は、生成 IR の `@malloc`／`@realloc`／`@free` と os.c 自身の libc 呼び出しを数える関数へ置き換え、ASan／UBSan 付きで 64 回繰り返して、どちらの生存数も 0 に戻ることを検査します。OS API の性能は測定しておらず、主張もしません。

**定数:** `Program.constants` を内部の引数なし宣言として収集し、関数と同じ名前・可視性・型検査を使います。
特殊化・所有権検査の前に依存を明示スタックで辿り、評価結果をキャッシュして参照を型付きリテラルへ展開します。
通常の型検査は両分岐を検査し、評価器だけが短絡します。APFloatでbinaryの幅ごとに計算し、decimal演算は拒否します。
内部宣言は非公開にして出力のrootから外します。独立した定数ランタイムや共有所有領域は作らず、既存のstring global・aggregate生成・frame/relocate/dropを共有します。
定数と一時値（呼び出し・パイプの結果）を共有借用の引数へ渡すときは、`constants::temporary_borrows` が `BorrowOperand` にして生成・呼び出し後解放し、型検査で単一段階の完全適用とloanを運ばない結果を要求します。
検証は `cargo test --locked --test constants` と `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri constants` です。

**デバッグ情報:** `llvm::emit_with_debug_info` は既存の `TrapSource` source mapを明示的に受け、通常APIはデバッグ情報を生成しません。
DIFile/DICompileUnit/DISubprogram/DILocationと変数・型を `Globals.next_metadata` で採番し、loop metadata・trap marker・同梱runtimeと範囲を共有します。
式位置は既存current_spanを使い、switchのcase行ではなく命令終端へ付けます。ローカルのentry alloca後にdbg.declareを置きます。
公開ABI/console wrapperにもスコープを付け、O3で内部関数をinlineしてもソース情報を保持します。生成関数名は親と既存の決定的な内部名を併用します。
binary floatは幅どおりのDW_ATE_float、decimalはBID storageをDW_ATE_unsignedとして記録します。unionの詳細payload表示は対象外です。
Clangに-gを渡し、WASMは--strip-allを外します。macOSのdebug task objectは対応するllvm-linkでIRを結合してから一つのobjectへ生成し、ld -rでDWARFが失われる問題を避けます。
macOSのdebug executableは保持したmodule.oからlinkし、一時領域を消す前にdsymutil --flatで隣接するoutput.dwarfを生成します。
DWARFとtrap表は共通のsource保護・backup・公開失敗時rollbackを使います。複数ファイルのcrash-atomic更新ではありません。
Cargo release profileのstripはコンパイラ自身だけに適用されます。検証は `tests/debug_info.mjs` のnative/WASM O0/O3とllvm-dwarfdump --verifyです。

**言語サーバー:** `tsuzuri lsp` はLSP 3.17のstdio framingとserde_jsonを使い、メッセージ16 MiB・header8 KiB・JSON深さ128を上限とします。
UTF-8位置を交渉し、既定はUTF-16です。byte spanからの変換は改行表と文字境界を使い、CRLF・補助平面の文字を保持します。
開いた文書のURIはクライアントから受信した表記を保持し、診断と同一文書への定義位置にそのまま返します。
`Project::load_with_overlays` は保存前の置換と新規ファイルを含め、root配下のプロジェクトを再解析します。ソースへの書き込みは行いません。
LSPのworkspaceFolders/rootUriを明示rootとして使い、指定がなければ各fileの親を使います。ネストしたroot指定は最長prefixを選びます。
変更を200msで集約し、問い合わせ時は必要な解析を先に完了します。開いたバッファは合計32 MiB・1024ファイルまでです。
reader threadは上限付きキューへ受信し、キャンセルIDを共有します。解析前・応答前にキャンセルを確認し、stdoutにはframed JSONだけを出します。
`analyze_modules_with_semantics` は通常の解析結果と `SemanticIndex` を返します。型検査後、定数展開・単相化・closure lowering前に採取し、通常解析では採取しません。
後続の所有権検査などが失敗した場合もindexを公開せず、以前の成功結果を破棄します。hover・定義・symbol以外のcapabilityは宣言しません。
検証は `cargo test --locked --test lsp` と `cargo build --release --locked && node tests/lsp_sessions.mjs target/release/tsuzuri` です。

**フロントエンド:** 全ファイルのシグネチャを先に収集するため、宣言順・ファイル順に依存しません。
ローカル束縛は一意な ID に解決します。コード生成時に名前解決や型推測をやり直しません。
呼び出し引数の借用・参照外しの補完（後述）を除き、暗黙変換、未知の名前への既定値、未対応構文の読み飛ばしは行いません。
決定性が必要な名前集合は順序付きコレクションです。
Span はソース ID とファイル内バイト位置を保ち、字句・構文・型・レイアウトのエラーを
元のファイルに対応付けます。ソースの連結や診断オフセットの書き換えは行いません。

`analyze_all`／`analyze_modules_all`／`analyze_modules_with_std_all`／`Project::analyze_all` は
`DiagnosticSet` を返し、従来の API はこの集合の先頭一件を返すラッパーです。
収集用 `Diagnostics` は `(source.unwrap_or(root), start, end, severity, code, message)` の順序付きキーで重複除去します。
収集上限 1000 件と表示上限 50 件を分け、打ち切り前なら省略数を正確に数え、収集を打ち切った場合だけ下限とします。
表示用の note は Diagnostic／診断コードではありません。user／std の source ID の順序は E02 のまま維持します。

警告は型検査後・特殊化前に `CheckedModule.warnings` へ集め、ソースエラーがある場合は表示しません。
`Ident.provenance` を `Local` へ引き継ぎ、W1001 はローカル ID の使用、W1002 は再帰検査と共有する呼び出しグラフと型参照の到達性で判定します。
型別名は消去前の注釈も走査します。std と生成束縛を抑制し、生成名の文字列から由来を推測しません。
W1003 は既存の網羅性解析、W1004 は既定無効の字句スコープ検査です。`--deny-warnings` はコード生成前に失敗させます。
W1006 は型検査の後段で作ります。`copies::sites` が具体化・クロージャ降格後の module で所有権検査器を収集モードで再実行し、
消費する読み取りのうち drop が必要な Copy 値の複製（単一使用の local の move を除く）を位置・型・種類と共に返します。
`main.rs` は `--warn implicit-copy` のときだけその配列・リストの複製を警告にし、LSP は同じ一覧を `textDocument/inlayHint` で返します。
debug build の LLVM emitter は利用者の式から生じた暗黙の `clone_value` を記録し、すべてが一覧にあることを生成の最後に検査します。

`Program.tests` は通常の名前空間と分離し、検査時に unit の内部 callable と `CheckedModule.tests` のメタデータへ変換します。
`FunctionOrigin.test` を局所生成関数へ引き継ぎ、共有特殊化は一つだけ生成して root 集合からの到達性で選択します。
通常出力は public 宣言・export・entry、テスト出力は選択したテストを根にし、同じ `reachable_functions` を使います。
内部 callable は既存の `@tz.fn.Module.$test.index` の命名規則を使い、runner だけが `tsuzuri_test_count`／`tsuzuri_test_run` を公開します。
native の C entry は `strtoull`・errno・endptr で index を厳密に検査し、WASM の Node entry は imports が空であることも検査します。
`driver::run_tests` は一度だけ runner をビルドし、上限付き worker が各テストを別プロセスで実行します。30秒 timeout では停止して wait し、結果を元 index 順へ戻します。

Debug.print/trace は通常の std 関数で、Display の所有 string を private builtin へ渡します。
native は既存の厳密な UTF-8 変換と `runtime/debug.ll` の write(2) ループを使い、変換前後の所有バッファを解放します。
WASM の既定は UTF-16 の表示結果を解放するだけです。opt-in は UTF-8 を `tsuzuri_debug.write` へ同期転送して解放し、ホストに改行を委ねます。
新しい `llvm::EmitOptions` と `emit_with_options` が出力フラグを受け、従来の `emit`／`emit_target` は既定値の wrapper として維持します。

`llvm::emit_with_trap_info` は source map を明示的に受け、IR と TrapSite を返します。既定 API では計測を行いません。
有効時だけ生成 call/trap に一時的な source marker を付け、`llvm_traps.rs` が生成 IR の関数・呼び出し・括弧付き引数を走査します。
内部 callable と同梱 runtime の最後の引数へ context ID を追加し、間接呼び出しも同じ ABI にします。公開 export・main・pthread callback は元の ABI を維持します。
標準 helper は呼び出し元 context を伝播し、利用者関数は実際の式 span を採用します。失敗理由ごとの ID は context と enum の offset から作ります。
soft numeric の生成済み IR もこの有効時だけの変換対象であり、numeric.c／numeric.ll の既定 ABI は変更しません。
失敗 block だけが cold reporter を呼び、native は write(2)、WASM は internal global と getter を使います。成功前の current-site store はありません。
変換で副作用が変わる helper/call の attribute group は引き継ぎません。source marker は最終 IR から除去します。
driver は side table を一時領域へ作り、ソース保護を再検査し、成果物公開の失敗時には表を rollback します。二ファイルの crash-atomic 更新ではありません。

Option 部分認識器と複数ケース全域認識器は、通常の呼び出しを `PatternStep::Bind` で保持し、既存の union case の tag test と payload 投影へ下げます。
複数ケースの `def ... -> 'T = ラムダ式` は parser が認識器専用の UnionDecl を生成します。Checker は本体内の case 名を通常のコンストラクタへ解決し、nullary case も参照のたびに値を作ります。内部名には利用者が書けない文字を含め、型・case の名前空間を混同しません。
payload の有無は本体内の case の直接適用から取り、型は既存 Checker による本体検査で確定して関数の scheme へ反映します。別の型推論器や union runtime は追加しません。
生成 union は API 文書と document symbol から除外し、API 署名には元の `'T` を表示します。
失敗したパターン・ガードの一時結果は既存の cleanup を使い、追加の所有権規則は導入しません。
名前表は関数 ID と case 種別だけを保持し、payload 型は使用ごとに fresh instantiate した signature から得ます。
複数ケースの CoveragePat は認識器・case index・case 数を保持しますが、再評価を伴う現在の意味では保守的に opaque とし、フォールバックを要求します。

`lex_all` は不正な一文字・数値・文字列から復帰し、未終端コメントは EOF で止めます。
formatter 用の `lex_with_trivia` は同じ token/span と、トークン間の空白・改行・コメントを保持します。
`formatter::format_source` は AST の型・単項演算子・本体終端・分岐の位置を利用して空白だけを整えます。
整形前後の AST を位置と depth を除いて比較し、位置由来の生成名は provenance に基づいて正規化します。
CLI は全入力の parse/整形を確認してから、同じ親ディレクトリの一時領域へ書き込み・flush・rename します。
`--check` と不変のファイルは書き込まず、symlink を拒否し、通常ファイルの mode を維持します。
字句エラーのあるファイルはその字句診断だけを返し、欠けた token から構文エラーを増やしません。
`parse_with_source_all` は宣言単位で回復し、消費済みの token も含め `()`・`[]`・`[| |]`・`{}` を追跡して、
深さ 0・column 1 の def／fn／record／union／class／instance／export／private／let だけで再同期します。
and は回復境界ではありません。失敗した宣言は定義名集合に登録せず、構文エラーが一件でもあれば AST を返しません。
全ソースの構文診断を先に集め、壊れたプロジェクトを型検査しません。

型検査は名前空間・ファイル種別・型宣言／レイアウト・クラス／インスタンスを段階ごとに集め、
構造が壊れた段階では停止します。全関数名を登録した後、シグネチャを個別に解決します。
不正なシグネチャは名前と ID を残し、返却型が `Type::Error` の poisoned Scheme として表します。
関数参照と依存する式は `TypedExprKind::Error` を伝播し、poison に触れた本体の未確定制約・網羅性・二次診断は出しません。
本体と entry だけで `Checker::expression`・`argument` の失敗を回復し、ブロック注釈・サイズ検証も個別に回復します。
回復時は `RecoveryMark` のスコープ長・ループ深さ・計算式深さを復元します。回復中の erroneous な期待型は「不明な文脈」として渡し、
兄弟式・match の節・呼び出せない対象の引数も検査します。空のコレクション・型なしリテラル・lambda の引数・汎用の引数など文脈で型が決まる部分だけを Error にし、
依存する式は既存の `finish_expression`・`contains_error` で抑制します。ブロックは型だけを `Error` にして正常な文を保持します。
大きい結果分岐は非再帰の `finish_recovery`、失敗処理は cold な `recover_expression` に分離します。
二項式も専用 dispatcher にし、深い式で汎用の値検査フレームを保持しません。`E1017` と収集上限では本体を打ち切ります。
型エラーがあるときだけ、宣言 ID と添字をそろえた `recovery_module` を構築します。検査できない本体は closed な Error stub にし、
型確定に失敗した本体も Error 本体にし、`ownership::check_recovered` が infer mode で全関数を検査します。
Error stub は loan を持たないため、この pass は access の診断だけを残し、寿命などの `Err` と Copy 制約は捨てます。
所有権の `access` は root ごとの最初の診断を保存し、診断済み root に起因する寿命・移動後の参照外しの二次診断を抑制します。
入れ子の本体も回復済み診断を共有します。その他の所有権エラーは本体を打ち切ります。
他の有効なシグネチャの本体と entry は独立して検査します。エラーがあれば意味索引・特殊化・closure lowering へは進めません。
エラーのないプロジェクトでは従来どおり Copy 制約を推論し、特殊化後に `ownership::check_all` を行います。
Type／式の Error は検査中だけの状態で、成功した CheckedModule には存在せず、LLVM 入口も明示的に拒否します。

**借用の表記:** キーワードの `ref`／`ref mut`／`deref` と Rust 互換の `&`／`&mut`／`*` は、構文 AST の
`Notation` だけが異なります。型検査で同じ型付き IR の `Borrow`／`Dereference` へ下げ、所有権検査と LLVM は表記を参照しません。
両表記の生成 IR が一致することを `tests/borrow_syntax.rs` で native／WASM について検査します。
キーワードの `ref` は被演算子を期待型なしで型付けし、静的な型が参照なら `Dereference` を挟んで一段の貸し直しにします。
型変数は貸し直しません。型が `Infer` の被演算子は `undecided_borrows` に記録し、`finish` の既定型の適用後に
参照へ解決されていれば `E1015` にします。推論結果から型付き木の形を後で変えることはしません。
字句解析は `ref`／`deref` を予約語にします。引数位置の記号は、直前の空白と被演算子の隣接で
前置の引数（`f &x`）か二項演算（`a & b`、`a&b`）かを決めます。

呼び出し・`|>`・認識器の追加引数は `Checker::argument` で期待型に合わせて借用・再借用・参照外しを補います。
名前・フィールド・索引・参照外しは元の型で検査してから、既存の `Borrow`／`Dereference` へ変換します。
引数に置いた呼び出しは `call_expression` で、結果の参照の段数・可変性を保った期待型を伝播してから補完します。
これにより数値リテラルやジェネリックな結果の文脈依存の型推論を保ち、引数を二度検査・評価しません。
明示的な `Borrow` の型はそのまま検査し、通常の束縛・戻り値や未確定の汎用値引数へ借用を推測しません。
参照型が既知の引数では排他参照を move せず貸し直し、非 Copy の参照先を値渡しできるようにはしません。
所有権検査と LLVM は明示形と同じ木を処理するため、競合・寿命・Capture・評価順序の規則を共有します。
引数検査の入口は小さな振り分けにし、補完の作業領域を再帰フレームに保持しません。

**制御構文:** while、整数範囲、コレクション反復、match を専用の型付き IR として保持します。
ループ・条件・パターンの型は型検査で確定し、LLVM で型推論やイテレーターの動的探索を行いません。
タプルは構造的な値型で、型置換・レイアウト・Copy／Capture／Send・loan・clone／drop へ再帰的に参加します。
型付き式の子の列挙を共有し、新しい制御節内の型置換・lambda lifting・使用数・呼び出し特殊化を漏らさないようにします。

再帰指定は構文 AST に保持します。型検査後・単相化前の参照グラフを反復的な SCC 走査で検査し、
自己・相互参照・関数値・認識器・インスタンスのメソッド／演算子の循環に `rec` を要求します。
抽象型のクラス呼び出しは該当クラスの実装を保守的に候補とします。
`def ... = ラムダ式` は従来の署名と実装を結合する `Parser::define` を共有します。`def rec ... =` と型付き `and ... :: ... =` は同じ再帰グループを持ちます。
分離形式の `def rec`／`def and` と実装のグループも一致させ、非再帰の前方参照は維持します。

ラムダ直後のガード節は既存の `guarded_definition` を共有し、`MatchOrigin::FunctionGuard` の通常の match に変換します。
後置 `where` はパーサーで通常の `Block` の束縛へ変換し、結果式にその match を置きます。新しい型付き IR や runtime は追加しません。
初期化式は最初のガードより前に記述順で一度ずつ評価し、型推論・スコープ・所有権・解放はローカル `let` と共有します。
`where` は節と同じインデントの行にだけ認める文脈キーワードで、lexer の予約語は増やしません。

パターンは左から順序付きの `Test`／`Bind` 手順と、成功時の束縛元へ展開します。
OR は束縛名・型を揃え、最初に成立した側でガードを一回評価します。
ガード失敗は次の節へ進み、同じ OR の別の側を再試行しません。
構造の長さ検査は要素ロードより前で、共有参照を辿る場合も既存の loan 規則を使います。
単一ケースの全域認識器は結果を一時ローカルへ保存し、部分認識器は bool の Test です。
検査中は読み取り専用の別名を使い、ガード成立後にだけ通常の所有ローカルへコピー／move します。
参照・コレクション要素・tail からの束縛は `TypedMatchArm.borrowed` に記録し、非 Copy で loan を含まない
具体型は成立後もビューのままにします。OR のどちらかがビューならその束縛を両側でビューに揃えます。
ownership はビュー自身を仮想の場所の根にし、元の領域へ親 loan 付きの共有 loan を保持します。
これにより派生借用の生存中も元の領域を保護し、ビューからの借用を節の外へ返せません。
抽象型がコピーされる必要のあるアクセスは `infer_copy` で Copy 制約を推論し、具体化した Copy 値は従来どおり成立時に複製します。
LLVM はビューをガードと同じ射影ポインターのまま読み、move／drop せず、元の所有者だけが解放します。
どの Test・ガードで不成立になっても、それまでの認識器の一時所有値を解放します。
一時スロットを entry に置き、別の OR 経路の未使用スロットはゼロにして drop を安全にします。
全節が不成立なら `llvm.trap`／`unreachable` で、成功形の既定値は生成しません。
OR 展開は最大 1,024 通りで、構文・展開後の深さにも既存の 128 上限を適用します。

網羅性（`exhaustiveness.rs`）は、`control::pattern_alternatives` が lowering と同じ名前・認識器の解決から
同時に返す型付きの `CoveragePat` を使い、構文 AST を再解決しません。
記録するのは `MatchOrigin::{Explicit, FunctionGuard}` の match だけで、ラムダ式・source の `for`・
コンピュテーション式の分解は検査せず実行時のトラップを維持します。
定数の鍵は既定の型を確定した後の型に依存するため、検査は関数ごとの `finish` の後に行います。
鍵は解決済みの型と実行時の等価性に合わせた正規形（整数のビット、浮動小数点数の ±0 の同一視と NaN の空パターン、
decimal の cohort の正規化）です。
usefulness は行列の特殊化で求め、`Rc` の永続的な行を反復的な DFS で辿るため、ネイティブのスタック深さは入力に依存しません。
完全なシグネチャは bool・unit・タプル・レコード・union の case・リストの空と cons で、
整数・浮動小数点数・decimal・文字列の定数と配列の長さは常に不完全として既定行列へ進みます。
部分認識器と結果のパターンが網羅的でない全域認識器は、網羅の行には何も加えず、到達可能性の問い合わせでは任意の値とみなします。
全列がワイルドカードの行を含む部分問題は網羅済みとして探索を打ち切ります。
一つの節の正規化が 1,024 通り、または探索の作業量が 2^24 を超えれば `E1017` にします。
非網羅は最初に到達した match の `E1021`、到達不能な節は arm のパターン位置の `W1003` です。
網羅性を保証した後も、LLVM の失敗ブロックの `llvm.trap` と `switch` の既定先は防御として残します。
警告は `CheckedModule.warnings` をソース・位置順に並べ、単相化・closure 変換を経ても保持して CLI が
`Project::source_for` の該当ファイルで表示します。

所有権の反復解析はループ入口・条件・本体・バックエッジの move と loan の合流を固定点まで検査します。
`break`／`continue` は unit 型の専用 IR です。型検査の通常ループ文脈は lambda・task・ビルダー境界で切り、
ビルダー展開後には深さを増やさない内部の `ComputationBoundary` を残して外側ループへのジャンプを拒否します。
所有権解析は到達可能な通常継続、break、continue の状態を分離し、後二者の一時ローカルを除去して借用を検査します。
通常継続と continue だけを固定点のバックエッジへ、条件 false と break を出口へ合流し、各反復で辺の収集をやり直します。
辺の収集はループごとに最大 4096、固定点の回数は従来の上限を保ちます。上限超過は `E1017` です。
LLVM はループごとの分岐先・ローカルと評価済み一時値の深さを保持し、ジャンプ前に内側の所有値を解放します。
部分的に初期化した配列・リストは初期化済みの接頭部だけを解放し、未初期化領域を読みません。
所有値を呼び出しや格納へ渡すと一時値の追跡から外します。ジャンプ後は到達不能ブロックを開始し、既存の phi 生成を維持します。
新しい反復のローカルは再初期化し、外側に出した参照が反復ローカルを指す場合は拒否します。
状態比較は loan の生成 ID ではなく参照先と可変性を比較し、上限を超える解析は `E1017` にします。
ループ内の一回の構文上の使用を「実行時にも一回」とみなして所有領域を奪ってはいけません。
反復元は一度だけ評価・読み取り借用し、配列・文字列は連続走査、リストはリンクの O(n) 走査です。
string は i16 の UTF-16 コード単位、utf8string は従来の i8 の UTF-8 バイトを直接ロードします。
添字を増やしてリストの先頭から繰り返し探索したり、列挙のために全体を深くコピーしたりしません。

狭い整数の単位刻みは i64 の誘導変数へ拡張し、終点の次の値も表現可能にします。
本体に入った値が元の幅に収まることだけを `llvm.assume` へ伝え、ユーザーの算術に overflow フラグを付けません。
64／128-bit の単位刻みは終点を処理した時点で停止し、任意刻みは overflow intrinsic と範囲比較で停止します。
密な整数の定数結果は最大 256 要素・密度 1/2 以上の静的表、それ以外の整数定数選択は switch、
構造・ガード・認識器は順序付き分岐へ下げます。表の範囲外も明示的な既定節へ進みます。
代入が一箇所だけの小さい整数 reduction には LLVM の unroll ヒントを付けますが、固定の展開数・ISA・再結合を強制せず、
浮動小数点 reduction やループ依存の整数ミキサーには適用しません。
ループ用 metadata は同梱数値ランタイムの ID 範囲と分離し、全 IR を決定的に生成します。

**コンピュテーション式:** `.tc` の全関数名を順序付き集合に収集し、ファイル名をビルダー名にします。
match!はBind+通常match、and!はsourceを先に順序付きの生成letへ保持してからMergeSourcesの左結合+Bindへ展開します。
構文的に最後の2文がlet!+return、または2要素and!+returnのときだけ、存在するBindReturn/Bind2を選びます。Bind2のcontinuationは2引数を同時に束縛し、mut/注釈を保持します。
追加した解析・展開は専用helperへ分離し、既存の深さ128と2 MiBテストスレッドのスタックを維持します。新TypedExpr/runtimeは追加しません。
使用側の `Builder { ... }` は元の Span を持つ専用の構文 AST とし、各関数・エントリーの型検査前に
`Bind`／`Return`／`ReturnFrom`／`Yield`／`YieldFrom`／`Zero`／`Combine`／`For`／`While`
への通常の関数呼び出しと匿名関数へ展開します。`Delay`／`Run` は存在するときだけ使います。
継続は残りの文を内包し、`let!` の注釈と `do!` の unit 制約を生成したローカル束縛で検査します。
注釈なしの `let!` は直接継続の引数へ束縛し、別名の束縛による余分な配列・関数環境のコピーを作りません。
単なる `let` と unit 式は同じブロックにまとめ、フラットな束縛列で不要な再帰を増やしません。
内側の式から展開し、通常の親式も含めて深さを再計算します。個々の展開だけを検査して
入れ子の遅延・継続の合計深さを見落とすと型検査のスタックが枯渇するため、全体で上限を維持します。
型・所有権検査では呼び出し・匿名関数・ブロックを値／演算の大きな分岐処理から分離し、
上限内の継続を検査するときも各再帰段に大きなスタックフレームを保持しないようにします。
生成名にはユーザーが書けない `$` を使い、操作名はローカル変数を経由しない修飾参照にするため、
ユーザーの `Bind` やビルダーと同名の束縛で展開先を変更できません。

型付き IR 以降にはカスタムビルダー専用の値・命令・ランタイムを追加しません。
通常の型クラス制約、特殊化、Capture、関数環境の複製、寿命と move／drop がそのまま適用されます。
操作の不在は `E1018`、型・所有権の不整合は既存の診断で拒否し、成功形の既定実装へ置換しません。
`Delay` なしでは `Combine` の両引数は厳格評価、ありでは第二引数の本体を遅延値に渡します。
`While` は bool を返す unit 関数と明示的な `Delay` の結果を受けます。
空の `Name {}` は収集済みビルダーを優先し、それ以外は既存の空レコードです。
組み込み `task` の cold・非 Copy・Send・一回消費の経路は別のまま維持します。
カスタム展開によって通常の関数値へ一回実行タスクや排他参照を捕捉させる特例は導入しません。

暗黙本体は同じComputation ASTにユーザーが書けない生成名を付けて保持し、型検査時に段階的に展開します。
結果型または最初のbind入力型と公開操作のsignatureから候補を選び、候補照合には既存Inferenceの独立コピーを使います。
候補照合で本体を再型検査せず、実際に検査した右辺は生成localへ一度だけ保持します。曖昧性はE1018です。
通常のprefixと型決定に使った最初の右辺も、選択したDelay/Taskのclosureの内側に置きます。
同一builderは従来の操作、異種は外側Returnによる入れ子、Usingがあれば通常の高階関数による合成です。
Usingへ渡すsourceは異種builderのBind/Return/Delay/Runを共有loweringで組み立て、IOはそのcallbackのIOを実行時に消費します。
closureの捕捉・Capture/Send・サイズ検査はchecked_lambdaを共有します。専用TypedExprやruntime interpreterは追加しません。
大きなAST構築helperを再帰型検査から分離し、暗黙展開にも既存の深さ128制限を適用します。
引数なし・非再帰・署名なしmainはparserで既存entryへ変換し、他モジュールやトップレベルとの併用は既存の入口検査で拒否します。
tests/computations.rsとcomputations.mjs、io.mjsで型・曖昧性・短絡・cold・所有値解放・native/WASM O0/O3を検証します。

**多相性:** 明示シグネチャの型変数は rigid、各関数参照で導入する推論変数だけを単一化します。
occurs check と型の深さ・構成要素数の上限を適用し、型が決まらない関数値は拒否します。
型クラスの要件は演算・メソッド・所有権から収集し、呼び出しグラフ上の固定点まで伝播します。
クラスとheadの単一化可能性でoverlapを拒否し、型別名も正規化した後に比較します。
インスタンスの本体は未使用でも通常の関数と同じ検査を受けます。
ジェネリック本体も宣言時に抽象型で検査し、特殊化時には型・所有権・寿命・レイアウト・
リテラル範囲の具体的な条件を再確認します。テンプレートを呼び出し時だけ検査する方式ではありません。
特殊化は `(宣言 ID, 型引数)` のキャッシュとキューで行い、再帰にも同じ ID を使います。
型変数、未解決のメソッド、未エンコードのリテラルは LLVM に渡しません。
ランタイム辞書・仮想呼び出し・汎用値の boxing は不要です。
旧宣言構文は互換テストとして残し、新しい例は分離シグネチャと空白適用を使います。

`ClassDecl`は既存のsignature列にsuperclassとdefault定義列を加え、名前で関連付けます。
`Classes.instances`はhead・context・メソッドIDと型引数を持つ一つのtemplate集合です。
別のconcrete instance cacheは作らず、既存Specializerの`(関数ID, 型引数)`キャッシュを使います。
headの型変数だけをfreshな推論変数にし、要求側の抽象変数はrigidに保って照合します。
`normalize`はsuperclass・条件付きinstance・構造比較の要素要件を反復的に展開し、残余制約を固定点へ渡します。
activeとcompletedを区別し、循環を成功扱いせず、重複要件だけを除きます。深さ64・異なる要件128・overlap1024組を上限とします。
デフォルトは宣言モジュールの汎用`$instance.default`関数を一つ合成し、未使用でも通常の型・所有権検査を行います。
本体から増えた制約はclass／instanceの明示contextで証明できる必要があります。
抽象段階の保守的な再帰候補に加え、具体化後も同じSCC検査を使い、型が縮むtemplate展開と実際の循環を区別します。

条件付きEq/Ordは通常の単相化済み関数の内部に`StructuralCompare`を持ちます。
子には左右の共有参照と具体的な要素メソッド参照を保持し、通常の到達性・型走査を共有します。
LLVMでは配列の添字、リストの2本のnode phi、タプルのfield GEPを使い、最初の不一致で短絡します。
各要素は既存の借用ABIを使い、部分適用のメソッドは残りの引数も順に適用します。辞書・汎用boxing・要素コピーは追加しません。

**導出:**
`derive::instances`はrecord/union宣言から通常の`InstanceDecl`／式／パターンASTを合成し、手書きinstanceと同じcoherence・型検査・所有権・特殊化へ通します。
生成名と内部クラス参照はprovenanceで区別し、同名モジュールへ誤解決しません。診断はderiving指定のspanを維持します。
派生した構造要件に限って同じクラスの再帰を許し、Defaultの循環は拒否します。一般の循環instanceは引き続きE1017です。
Hashのプリミティブは幅別の整数load/shift/xor/wrapping multiply、decimalはnumeric.cのdecode/encodeを共有します。
`StructuralHash`／`StructuralDisplay`は合成されたコレクションhelper内で使い、子の具体メソッドを通常の到達性走査に含めます。
表示は内部DisplayQuoted builtinを具体型へ解決し、`runtime/display.ll`がUTF-16引用と部品の一括結合を行います。D02のstdソースには依存しません。
Hashのcanonical streamと文字型別の引用規則は言語仕様を参照してください。numeric.llは生成器から再生成し、手編集しません。

**文字列補間と書式指定:** `$"a{x}b"`／`u8$"..."` は lexer が `InterpolationStart`／`InterpolationMiddle`／`InterpolationEnd` の token に分け、parser が `ExprKind::Interpolated`（文字列片と `InterpolationHole { value, spec }`）にします。lexer は開いた穴を `holes`（`OpenHole { utf8, depth, start }` のスタック）で持ち、穴は括弧の深さが 0 の `}` か `:` で閉じます。穴は同じ行に置く必要があり、コメントは使えません。穴のないリテラルは通常の文字列 token のままです。限界は 1 リテラルあたり 1024 の穴、幅と精度は 4096、入れ子は parser の深さ制限です。
型検査（`interpolation`／`interpolation_hole`）は `TypedExprKind::Interpolated(TypedInterpolation { texts, holes })` を作り、型はリテラルの接頭辞に従う `string` か `utf8string` です。`TypedHole.operand` は必ず `ref U` です。場所（名前・フィールド・添字・参照外し）は `ref` 引数と同じく借用し、参照はそのまま使い、それ以外の値は `BorrowOperand` の一時値にして、文字列を結合した後に drop します（引数なしの呼び出しや union の case 単体のように値を作る名前も一時値です）。穴は左から右へ一度ずつ評価し、値は消費しません。`method` は `Display.display`（`custom` の穴は `Format.format`）で、リテラル自身の文字列型の穴と、符号・精度・type を持つ数値の穴には持ちません。
LLVM（`llvm_display.rs`）は、全ての穴を左から評価して `Piece`（データ・長さ・所有者・ASCII か・padding）にし、リテラルの定数長と穴の長さと fill の単位数を足して、結果を `@tz.string.allocate`／`@tz.utf8string.allocate` で一回だけ確保します。文字列片は `copy` で書き、穴は表示した文字列のバッファから直接コピーします（リテラルと同じ型の文字列の穴は Display を呼ばずオペランドのバッファを読み、数値の ASCII は UTF-16 の結果なら `@tz.format.widen` で拡げながら書きます）。最後に一時値を解放します。`u8$` では Display の UTF-16 の結果を `@tz.utf8string.from_string` で UTF-8 へ変換し（孤立サロゲートはトラップ）、変換元を解放します。
数値の書式指定は `tz_soft_format_spec`（`src/runtime/numeric.c`）が、ホストの printf を使わず多倍長の係数から一度だけ最近接・偶数丸めして ASCII を書きます。`numeric.ll` は `python3 src/runtime/generate.py`、`math.ll` は `python3 src/runtime/generate_math.py` で再生成し、手編集しません。`flags` は `+` を bit 0、style（0 既定・1 `f`・2 `e`・3 `x`・4 `X`・5 `o`・6 `b`）を bit 4 以降に持ちます。バッファは `format_capacity` が型と精度から決める大きさの entry alloca（512 byte 以下）か、`@tz.alloc` の領域です。padding の補助関数は `runtime/format.ll` にあり（スカラー数を数える `@tz.format.scalars.utf16`／`utf8`、fill を書く `@tz.format.fill.u16`／`u8`、ASCII を UTF-16 へ拡げる `@tz.format.widen`）、前後の fill 数は LLVM が穴ごとに計算します。幅は Unicode スカラー数で数えます（サロゲートペアは 1、孤立サロゲートも 1、数値の ASCII は長さそのもの）。不足分は、既定で数値は右・それ以外は左に置き、`^` は不足の半分（切り捨て）を前に置きます。fill のスカラーは結果の符号化に詰め直した値で書きます。

**Format クラス（D07 Phase 2）:** `Format<'a>` は `BUILTIN_CLASSES` の最後（27 番目）に足した組み込みクラスで、メソッド `format :: ref 'a -> ref string -> string` は組み込みの実装（`Operation`）を持たず、利用者の instance だけが実装します。instance の頭は `validate_format_instance` が、このプログラムで宣言した record か union（型引数は自由）に限り、それ以外は E1016 です（穴が到達しない instance を作らせないためで、`validate_drop_instance` と同じ位置で検査します）。クラス名 `Format`（利用者の `record Format` は E1001）とモジュール名 `Format`（E1011）は予約です。`TypedHole.custom` は、spec があり、穴の型が record か union で、spec が符号・精度・type を持つか `has_format_instance` が真のときに立ちます。そのとき `method` は `Format.format` で、instance がなければ E1005（`no instance for Format<T>`）です。条件付きの instance（`instance Format<'a> => Format<Box<'a>>`）も使えます。`custom` でない穴の spec は `check_format_specs` が推論後の型に対して検査し（`+` は数値、精度は float と decimal、`x X o b` は整数）、符号・精度・type を持つ spec の穴が型変数のままなら E1003 です。Display しかない record・union に幅と配置だけを付けた穴は、従来どおり Display の結果を pad します。
`format_piece` は、借用した値と、検証済みの spec を `spec_text` の正規の綴り（既定の fill と省略された部分は書かない。例 `*>+8.2f`）にした文字列定数を、スタックの `%tz.string` descriptor 経由の借用として instance に渡し、返った文字列を穴の結果としてそのまま使います。コンパイラは padding も後処理もせず、幅・配置・fill は instance の責務です。`std/Format.tz` の `Format.parse :: ref string -> Option<Format.Spec>`（lexer と同じ文法で、不正な文字列・先頭が `0` の幅と精度（`.0` は可）・4096 超・lexer が拒否する fill（`{`・`}`・`"`・`\`・CR・LF、サロゲートペアの片割れ）は `None`。type と precision の組み合わせは検査しません）と `Format.pad :: ref Format.Spec -> string -> string`（幅をスカラー数で数える）はそのための通常の std ソースで、コンパイラは参照しません。実行時に組み立てた書式文字列、`deriving (Format)`、`#` と `0` の flag、locale、grapheme cluster 幅（D09）は未実装です。

**文字列補間の編集支援と検証:** VS Code の TextMate 文法は `string.interpolated.tsuzuri`・`meta.embedded.interpolation.tsuzuri`・`constant.other.format-spec.tsuzuri` の scope を付けます（`vsc/syntaxes/tsuzuri.tmLanguage.json`）。LSP は穴の式を型・定義の索引に含め、補間 token の文字列片と書式指定の位置では補完を返しません。formatter は穴の式の前後に空白を入れません。
検証は `cargo test --locked --test string_interpolation` と `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri string_interpolation` です。後者は spec の文法・padding・`Format` instance を、独立した JavaScript の参照と native／WASM の `-O0`／`-O3` で照合します。`tests/lsp.rs` と `vsc/src/test/grammar.test.ts` が編集支援を検査します。性能の主張はありません。

**関数シグネチャの制約行:** インデントした `@'T : Class, #function` を `ConstraintExpr` の
クラス名／関数名に区別して保持します。クラスは従来の `Constraint` へ下げ、インラインの `Class<'T>` と前置制約も維持します。
関数の要件は `MemberConstraint` に対象型、関数名、参照元モジュール、使用時の関数型を保持します。
`'T.function` の構文ノードは識別子を box にし、専用パーサーへ分離して既存の最大深さでのスタック使用量を抑えます。

関数本体を収集してから `solve_members` が関数要件を呼び出しグラフ上で固定点まで伝播し、
対象型がレコード・union に決まった要件を `Names::member_function` で定義元モジュールへ解決します。
必要な関数型と選ばれたシグネチャを単一化し、呼び出し元の返却型も確定してから `finish` と網羅性検査を行います。
数値の既定型は決定可能な関数要件を解いた後に適用し、残る要件をもう一度解きます。
関数要件は重複を除いて関数あたり最大 128、伝播した型にも既存の深さ・構成要素数の上限を適用します。
抽象的なメンバー参照の再帰検査は、同名の可視なモジュール関数を保守的な候補にします。
単相化で可視性・存在・関数型を再確認し、通常の関数参照へ変換してから捕捉・所有権・closure lowering を適用します。
`private` の検査は参照元モジュールを維持し、呼び出し元や具体型のモジュールへ権限を置き換えません。
型変数による関数参照を LLVM へ渡さず、専用のランタイムや動的探索は追加しません。

**整数 intrinsic:** `Int.*` は通常の多相 builtin として、各整数幅の LLVM intrinsic・命令へ下げます。
count のゼロ未定義指定は false、rotate 量は明示的にマスクし、checked 系は既存の Option union 構築を共有します。
i128 の checked／saturating 乗算は 64-bit limb の部分積と carry によって上位 128-bit の存在を調べ、
符号付きでは絶対値の限界も検査します。通常の i128 乗算補助だけを使い、未同梱の `__muloti4` を要求しません。
べき乗は二乗法で、最後の指数ビットの処理後に不要な二乗を行いません。速度の優位性は主張しません。

**消費型コレクション更新:** `Array.set`／`update`／`swap`、`List.cons`／`tail` は通常の多相 `BuiltinInstance` です。
既存の builtin ラッパーと emitter の境界検査・drop・関数適用を共有し、別の関数本体フックは持ちません。
呼び出し側の Copy／move とフレーム移送を済ませた所有バッファだけを更新し、builtin 本体で全体複製しません。
`update` の旧要素と関数環境は通常の所有するクロージャ適用へ渡すため、適用後に二重解放しません。
`tail` は次リンクを読む前に空を検査し、先頭だけを解放します。確保回数は storage E2E で単一使用・旧値再利用・スタック移送を区別します。

**遅延反復:** Seqはopaque標準recordで常にnon-Copyです。headはOption要素、stepはOption closureを持ち、空・onceは環境確保を必要としません。
Seq.nextだけは型付きbuiltinで表現を検証し、headを移送するか、step closureを所有モードで一度呼び出します。反復ごとの環境の全体cloneは行いません。
defer/unfold/map/filter/to_arrayは通常のstdソースで、Captureとloan伝播を共有します。filterは要素を失わないよう借用述語にします。
Seq forは型検査時に既存Block/While/Match/Assign/Breakへ展開し、毎回next stateを復元してからユーザーpattern/bodyを検査します。
生成名と完全修飾builtin参照を用い、ローカルのSeq名で展開先は変わりません。既存の固定点所有権・loop cleanupを共有し、直接collection forを変更しません。
Array/Vec/Map/Setは共有参照とindexでstepを構築し、Listはfold_refで参照Vecを一回作って所有状態として移送します。
検証は `cargo test --locked --test iteration_protocol` と `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri iteration_protocol` です。

**順序付きコンテナ:** Map/Map.Entry/Setをopaque標準recordとして登録し、構築・field・pattern・updateを定義モジュールに限定します。
型名は通常のgeneric recordで、Mapは`Vec<Entry<K,V>>`、Setは`Vec<K>`を保持します。Vecの非Copy・確保・clone/dropを共有し、新しいランタイム表現を増やしません。
lower_boundで借用比較し、更新はVec.push/pop/swapによる所有値の移動です。重複Map挿入は旧keyを保持してvalueだけ置換します。
Set.unionは末尾から比較・popして一つの逆順出力を作って反転し、intersect/differenceは前方の二本のindexで選択します。
opaque recordには共有参照を格納できますが、排他参照を拒否し、通常のrecordへ隠して格納する経路も検査します。
NLLのloan情報解放では、生きたloanの参照先と親を辿り、読み出しに必要な元所有者の情報を保持します。callbackの参照判定も実際の格納型を使います。
検証は `cargo test --locked --test map_set` と `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri map_set` です。

**ハッシュコンテナ:** `HashMap`／`HashMap.Entry`／`HashSet` を `Map` と同じ opaque 標準 record として登録するだけで、新しい `Type` variant・builtin・runtime 関数はありません（登録は `stdlib.rs` の予約名と opaque record の一覧）。実装は `std/HashMap.tz`／`std/HashSet.tz` の通常の Tsuzuri ソースで、`HashSet<'key>` は `HashMap<'key, unit>` を一つ持ちます。
`HashMap<'key, 'value>` は、挿入順に詰めた密な `entries: Vec<Entry>`（hash・key・value）、entry の添字か -1 を持つ open-addressing の索引表 `slots: Vec<i64>`、非公開の `key0`／`key1`／`keyed` からなります。索引は線形探査・2 の冪の大きさ（最小 8）・最大負荷率 1/2 で、`2 * (count + 1)` が大きさを超えるときに倍へ再構築します。`remove` は tombstone を作らず、理想位置との距離で後続の cluster を詰める後方シフト削除で索引を直し、entry 配列からは swap-remove（末尾の entry が空いた位置へ移る）します。したがって列挙順は insert と remove の列だけで決まり、hash 値・表の大きさ・target・seed のどれにも依存しません。
既定の hash は `Hash.hash`（64-bit FNV-1a）を `fmix64` で混ぜた値です。FNV-1a の下位 bit は入力の下位 bit だけで決まり、そのまま 2 の冪の表の添字にすると衝突が集中するためです。`with_seed`／`with_capacity_and_seed` の map は `keyed` を立て、seed から SplitMix64 で作った 128-bit の `key0`／`key1` を鍵にする SipHash-1-3（公開の `HashMap.sip13`。64-bit word 一つの純粋な関数）で、同じ digest を最終化します。`randomized`／`try_randomized` は `Random.next_u64` で seed を得る `IO` action なので OS API であり、既定の wasm32 は E2000、native と `--wasm-host wasi` で使えます。seed を得られなくても固定の seed へは戻りません。seed 付きの map は固定の混合に対して slot を狙った衝突を防ぎますが、`Hash.hash` の 64-bit digest 全体が衝突する key は衝突したままで、seed が推測できれば効果がありません。既定の map は HashDoS への耐性を持ちません。
キーを手放さない操作 `contains_key_ref`／`get_ref`／`at_ref`／`remove_ref`（HashSet は `contains_ref`／`remove_ref`）は `ref 'key` を取り、`at_ref {r s}` の region 契約により結果は map だけを借用します。key の `Eq` が非反射的（NaN）なら `hash_of` の assert でトラップします。`longest_probe` は理想の slot から最も遠い entry の距離を返す診断です。SIMD の群探査（F08）・縮小・集合演算は未実装です。
検証は `cargo test --locked --test hash_map` と `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri hash_map` です。seed 付きの container が OS runtime を要求せず randomized が要求することは `tests/os_api.rs`、native・WASI・既定 wasm32 での挙動は `tests/os.mjs` の `randommaps` が検査します。性能の主張はありません。

**共有配列ビュー:** `ref [T]` は `%tz.array = { ptr, i64 }` の非所有記述子で渡し、サイズは 16 バイトです。
`Vec<T>` は `%tz.vec = { ptr, i64, i64 }`（データ・長さ・容量）で、保守的な型サイズは 32 バイトです。
常に non-Copy ですが、捕捉環境の内部 clone は容量を保って独立複製します。drop は長さ以内の要素だけを解放します。
再確保は native realloc、WASM は隣接 free block の分割・吸収を試み、失敗時に領域を確保してコピー・解放します。
null／zero をヘッダー読み取りより先に扱い、WASM の上限（既定 16 MiB）は維持します。
`ref mut [T]` と他の参照は引き続き `ptr` です。型の共通レイアウト・閉包・引数・返却値もこの表現を使います。
`Slice` は元配列の loan を張ってから範囲式を検査し、LLVM は全境界検査の後にだけ GEP と長さの差を生成します。
共有配列参照の非消費な参照外しはビューを読み、所有値としての参照外しは従来の配列複製を使います。
要素の借用はビューから直接要素ポインターを計算します。パターンで全体の射影が必要な場合だけ entry の一時記述子へ保存し、
利用者にはこの記述子への可変アクセスを公開しません。スライス自体は clone・drop でバッファを操作しません。

**境界検査の省略:** `ranges.rs` は関数ごとに一度、型付き IR を worklist で走査して `RangeFacts` を作ります（節点が 65,536 を超える関数は事実なしで、検査をすべて残します）。
`Type::Array` の添字は、閉じた規則だけで必ず範囲内と示せたときに検査の分岐を省きます。規則は、`0 .. len - 1` と `len - 1 .. -1 .. 0` のループ（`len` は `.length` と std の `Array.length`）、定数端点のループ、リテラル・`new` による定数長、`if` の条件 `i >= 0 && i < len` の then 側です。
配列の局所変数は、関数内で一度だけ宣言され、代入も可変借用もされないときだけ安定とみなします。`Vec`・リスト・文字列、`ref mut [T]`、`Array.set` などの builtin、公開 ABI の入口は対象外で、検査を残します。
省略しても `getelementptr inbounds` と `load` はそのまま出し、`llvm.assume`・`!range`・`nsw`・`nuw` は新しく出しません。解析は LLVM より前なので `-O0`・`-O3`・native・WASM で同じ判断になり、省略した位置は trap 表に現れません。残した検査の `TrapKind::BoundsCheck` と位置は変わりません。
検査を省いた読み出しを含む整数 reduction のループには、`hint_loop` が `llvm.loop.unroll.enable` を付けません。検査が消えたループにこのヒントを付けると、LLVM がベクトル化より先に実行時展開して `array_index_sum` が遅くなりました（[実測](benchmarks.md#境界検査の残り方f12)）。検査が残るループのヒントは変えていません。
検証は `cargo test --locked --lib ranges`、`cargo test --locked --test bounds_checks` と `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri bounds_checks` です。

**比較の借用:** `Eq`／`Ord` のメソッド型は共有借用を受け取り、比較演算子の所有権検査は全型で `Use::Read` です。
非 intrinsic の演算子だけを `Call(method, BorrowOperand(left), BorrowOperand(right))` へ単相化し、
場所ならポインター、一時値なら `frame_value` の SSA 値を entry のスロットへ保存して渡します。
一時値は呼び出し後に左から右へ `drop_framed` で解放します。ソースの `Borrow` の制約は変えません。
数値・文字列の組み込み演算子は直接 lowering を保ち、メソッド値のラッパーだけが借用を参照外しして同じ演算を使います。

**型別名:** `Names.type_aliases` は宣言元・可視性・右辺の `TypeExpr` を保持し、レコード・union と
同じ型名解決を使います。`TypeAliasExpansion` は右辺と引数をそれぞれのモジュールで解決してから
型式を置換します。型クラスの適用も型式として保ち、`inline_constraints` と格納型の制約検査へ引き継ぎます。
循環は `E1024`、深さ 128・走査要素数 4096 の超過は `E1017` で拒否し、展開結果に `bounded_type` も適用します。
未使用の宣言も検査し、public 別名の右辺には既存の private 型漏れ検査を適用します。
`Type`・型付き IR・LLVM に別名専用表現を残さず、インスタンスと特殊化は展開後の型だけを使います。

**型パラメーターの構文:** レコード・union・型別名・型クラスの宣言、型の適用、制約・インスタンス、
`Task<T>` は共通の `<...>`・カンマ区切りで解析します。名前と `<` は隣接させ、
型引数内は完全な `type_expr` を読むため、入れ子の型適用・関数型・借用型に追加の括弧は不要です。
空のリストは拒否し、末尾のカンマは許します。個数と再帰の深さには既存の 128 上限を適用します。
型の閉じ括弧を読むときだけ `>>`・`>>>`・`>=` から一文字ずつ `>` を消費し、
トークンの挿入なしで正確な Span を保ちます。式の比較・シフトは従来のトークンのままです。
旧来の空白区切りの型引数は受理しません。暗黙の全称量化・推論・型付き IR・LLVM の正規名は変えません。

**ジェネリックなレコード:** 構文 AST の名前付きの型適用は `TypeExprKind::Apply(head, args)` 一つで、
`Add<'a>` のような制約と `Pair<i64, string>` のような型の具体化を型解決時に名前の種類で分類します。
クラス名・レコード名は一つの名前空間で、未知の先頭名・個数違い・非ジェネリック型への適用は `E1004` です。
型付き IR のレコード型は `Type::Record(id, args)` で、非ジェネリックなレコードは空の引数列を持ちます。
型引数は `Box<[Type]>` に保持して `Type` を 32 バイトに保ち、深い式の検査が 2 MiB のテストスレッドでも
スタックを使い切らないようにします。`TypeExprKind::Apply` も同じ理由で先頭名と引数列を box にします。
宣言のフィールド型は型パラメーターを含んでよく、`TypeContext` の `record_fields`／`record_field` が
実引数で置換したフィールド型を返します。Copy／drop／clone／loan／Capture／Send・所有権の部分 move・
呼び出し特殊化の可変性判定・フレーム配置は、宣言のフィールド型ではなく必ず置換後の型で判定します。
レイアウト（64 KiB 上限・再帰の検出）と参照を格納する具体化の拒否は具体型ごとに行い、
シグネチャ・型注釈・リテラル・単相化後の式型の各境界で再検査します。
LLVM では非ジェネリックなレコードを従来どおり `%tz.record.M.N` として宣言順に出力し、
続いて出力する全関数のシグネチャ・ローカル・式型と非ジェネリックなレコードのフィールドから
具体インスタンスを `BTreeSet<Type>` に集め、入れ子のインスタンスまで閉包してから一回ずつ定義します。
名前は `%"tz.record.M.Pair[i64,string]"` のような正規の型テキスト（`array[T]`・`list[T]`・`tuple[T,U]`・
`fn[T,U->R]`・`ref[T]`・`refmut[T]`・`task[T]`）で、ジェネリックな宣言自体の LLVM 型は出力しません。
集合の順序と正規名は入力順やハッシュに依存せず、native／WASM で同じ決定的な IR になります。

**レコード更新:** `{ base with field = value; ... }` は最初の式を通常のパーサーで読んでから残った `with` で判別します。
`match` 内の `with` をトークン走査で探しません。AST は `RecordUpdate` として元の値と更新フィールドを保持します。
元の値と更新値を記述順に保持する専用の型付き `RecordUpdate` に下げ、children の走査順と所有権検査も同じ順にします。
LLVM は元レコードと全更新値を先に評価し、置換する旧フィールドを解放してから `insertvalue` で差し替えます。
未置換のフィールドには通常のフィールド射影による Copy を行わず、元の aggregate を直接引き継ぎます。

**共用体（union）:** 構文 AST の `UnionDecl` は case ごとに 0／1 個の payload 型を持ち、`of` は
case 宣言の中だけの文脈キーワードです。union 名はクラス・レコードと同じ型の名前空間、case 名は
同じモジュールの型名・関数名・active pattern と衝突しない値の名前空間に入り、衝突は宣言順によらず `E1001` です。
型付き IR の型は `Type::Union(id, args)`、宣言は `CheckedModule.unions` の `CheckedUnion` で、
`TypeContext::union_payload(s)` が実引数で置換した payload 型を返します。Copy／drop／loan／Capture／Send は
レコードと同じく置換後の全 payload から構造的に決めます。
式は `Construct { union_id, case_id, payload }`（値の構築）、`CaseConstructor`（関数値として使う case）、
`UnionTag`（`i32` の tag）、`UnionPayload`（case の payload への射影）です。
完全適用された case は `Checker::call_kind` で `Construct` にし、呼び出しを残しません。
関数値として残った `CaseConstructor` は再帰検査・単相化の後に `closures::lower` が
`(union ID, case ID, 具体的な型引数)` ごとに一つの非公開関数 `$case.M.U.C[.$mono.N]` へ置き換えるため、
再帰の診断に合成関数は現れません。
case パターンは `Test(UnionTag(subject) == case)` と `UnionPayload` の射影に分解し、
ownership では `UnionPayload` を要素（`ELEMENT`）と区別した場所成分 `PAYLOAD` として扱います。
payload の move は union の部分 move で、移動元の payload 領域を 0 にするため、後続の union 全体の解放は
移動済みの payload を二重に解放しません。

保守的なレイアウト（`Layouts::union` と `llvm_frame::stack_size` で共有）は payload なしで 8 バイト、
それ以外は 16 バイトと最大の payload を 16 バイト境界に切り上げた値の和で、64 KiB 上限と再帰の検出（`E1010`）を
具体化ごとに行います。再帰する union は以下の所有ノード表現を使います。
LLVM の表現は `UnionLayout` で決めます。全 case に payload がなければ `i32`（`Enum`）、
全 payload が同じ LLVM 型なら `{ i32, T }`（`Common`）、それ以外は `{ i32, [K x i128] }`（`General`）です。
K は 64-bit ターゲットの正確な格納サイズ（i128 を 16 バイト整列、wasm32 以上）の最大値を 16 で切り上げた数で、
payload は領域への GEP と payload 型での load／store で読み書きします。
名前は `%"tz.union.M.Shape"`・`%"tz.union.M.Maybe[i64]"` で、ジェネリックなレコードと同じ
`named_instances` の閉包で具体インスタンスだけを決定的な順序で一回ずつ定義します。
解放・複製は tag の `switch` で所有値を持つ case の payload だけを処理し、非活性の payload を読んだり解放したりしません。
`Construct` はフレーム経路の対象外で、union の値は `new` なしのリテラルのフレーム領域を持ちません。

`recursive::analyze` は具体的な record/union と、tuple・array・list・Vec の格納辺を調べます。
function/task/reference は格納グラフの境界です。同一の具体型へ戻る循環とSCCを分類し、
有限値の固定点、union を通らない循環、型引数の成長・変更を検査します。
結果は宣言ごとの型引数キャッシュに保持し、元の宣言・`Type`・canonical name を書き換えません。
レコードは inline のまま、再帰SCC内の union のみ `ptr` に下げます。最初のnullary caseはnullです。
ノードは `{ next, drop_action, clone_action, tag, payload }` で、タグとpayloadは別のGEPで参照します。
構築子の直接適用と関数値は同じ `construct_value` を使い、payload 評価後に一回確保します。
部分 move は子の所有スロットをゼロ化し、親ノードは残ったpayloadとともに一度だけ解放します。

`runtime/recursive.ll` は共通の反復ループ、`llvm_recursive` は具体型ごとのstepを生成します。
drop は解放予定ノードの `next` を待ちリストに使い、子を登録してから親をfreeするため追加確保しません。
clone は複製先ノードを待ちリストに使い、一時的にstepとsourceをヘッダーへ保存し、
payloadを埋めた後で通常のdrop/cloneヘッダーへ戻します。別のwork itemは確保しません。
動的コレクションはその場で走査し、再帰する子は同じ待ちリストへ登録します。
LLVM-only helperは到達した関数の要求分だけ生成し、公開ABIや利用者の関数・警告・テストrootへ追加しません。
100万ノードのnative解放、WASMの上限内の深い複製と既定16 MiB超過トラップを検証します。

**利用者定義の解放（B07）:** 組み込みクラス`Drop`のinstanceのheadは`CheckedRecord::user_drop`／`CheckedUnion::user_drop`の印になります。
印はinstanceの収集直後、superclassの検査と関数本体の型検査の前に立て、`Type::has_user_drop`が`is_noncopy_record`（Copy判定の3か所が最初に見る）・`needs_drop`・`can_capture`へ伝えます。
単相化の後、特殊化済みの関数の型が所有するDrop型（field・payload・要素・Taskの結果。参照と関数型は辿らない）を集めて`drop`を特殊化し、
固定点の結果を`CheckedModule::user_drops`（`BTreeMap<Type, usize>`）に置きます。Drop instanceがなければ走査せず、IRも変わりません。
所有権検査は`Place::through_drop`でDrop型を通るfield・payloadの場所を記録し、そこからのmove、Drop型の`RecordUpdate`、`drop`本体の引数全体への代入を拒否します。
呼び出し先も置き換えうるので、`drop`本体では引数の値全体の排他的な再借用と、`ref mut`の引数そのもののmoveも拒否します（`Checker::drop_root`）。
LLVMはmove済みの領域をzeroinitializerで埋め、その解放を無処理にする規則を保ちます。そのため非再帰のDrop型はrecordのfieldの後（unionはtagとpayloadの後）に
`i8`の生存フラグを持ち、構築（recordリテラル・フレームの集約・`construct_value`）で1にします。全caseがnullaryのDrop unionは素のtagではなく`General(0)`です。
drop glue（`drop_value`と`drop_framed`）はフラグが0でなければ値をentryのslotへ置いて`drop`を直接呼び、読み直した値のfieldを宣言順に解放します。
zeroの領域はフラグも0なので`drop`を呼ばず、fieldの解放も従来どおり無処理です。一時値のCopyのfield・payloadを読むときは値を壊さず全体を解放します。
再帰するDrop unionは先頭のnullary caseもノードに置き（nullはmove済みだけを表す）、型ごとのdrop helperがノードごとに`drop`を呼んでから子を待ちリストへ積みます。
`user_drops`の関数は本体から参照されないので到達可能性の根に加えます。Drop recordはABIのscalar recordにしません。

`use`束縛は`syntax::Binding::using`を持つ`let`で、型検査が値の型に`Drop`を要求します。計算式の`use!`は生成した継続の引数を`use`で束縛し直し、`task`の`use!`は`let!`と同じ`TaskRun`です。
`Owned.drop`・`Owned.function`・`Owned.call`は組み込み関数で、`std/Owned.tz`はopaqueなnon-Copyの`Owned.Function<'a, 'b> { run: 'a -> 'b }`だけを宣言します（std関数を足すと生成関数の番号がずれ、既存のIRが変わるため）。
`Owned.function`の引数に直接書いたラムダは`TypedExprKind::Lambda::owned`になり、捕捉に`Capture`の代わりに`Send`を要求します（単相化後の再検査も同じ）。
`closures::lower`は持ち上げた関数に`owned_captures`を立て、`TypedExpr::consuming_use`で、Copyでない捕捉値（drop glueのない`extern type`のハンドルを含む）を本体がmoveしないことを確かめます。
LLVMはその関数の捕捉引数を借用ローカルとして生成し、applyは本体を呼んでから、消費する呼び出しのときだけ`@tz.env.drop`を呼びます。
環境のcloneは生成せず（記述子のclone pointerはnull）、`can_borrow`は偽にして既知のclosureの直接呼び出しと借用workerの特殊化に乗せません。
`Owned.call`は環境の借用（`i1 true`）でapplyを呼び、`Owned.Function`は`can_capture`を満たさず、`clone_value`へ渡りません。

match は `llvm_control::SwitchPlan` が、ガードのない全節の条件が単一の tag（整数・bool・unit の場合は値）の
定数比較で、束縛が同じ射影の経路を持つ場合に限り、対象の領域から tag を一度だけ load する `switch i32` にします。
束縛は `UnionPayload`／フィールドの射影のポインターから読み、対象全体の領域を別名にしません。
OR の各側の束縛経路は一致させ、条件を満たさない match は従来の順序付き検査に戻します。

**カリー化:** 型宣言は `def`、実装は `fn` または宣言に対応する `let` と lambda です。
型の矢印は右結合、適用は左結合です。内部の `Type::Function` の引数列は矢印列を正規化した
表現であり、非カリー化を意味しません。実装の `Signature.parameters` は各 worker が
実際に受け取る引数列を保持し、途中で関数を返す worker の評価を後続引数より後へ遅延しません。
完全適用された既知の worker は直接呼びます。部分適用・未知の関数値の適用は
`{ code, environment, clone, drop }` の記述子と一引数ずつの adapter を使います。
通常の adapter の内部引数順は値引数、環境、`i1 borrow` です。値の返却レジスターを次回の値引数へ使えるようにし、
ループごとの不要な移動を避けます。値引数のない thunk と一回実行 Task の呼び出し規約は変えません。
借用が残らないと証明できる途中段階では一時 loan を終了し、不明な場合は保守的に保持します。

**捕捉:** 自由変数を ID で求め、lambda を捕捉引数付きの worker に持ち上げます。
捕捉時は Copy／move、関数値をコピーするときは独立した環境スナップショットを作ります。
string と内側の関数値も環境ごとに複製するため、GC・参照カウント・共有可変状態は不要です。
通常の Copy 値にも関数値を含む場合は destructor が必要なので、`is_copy` と `needs_drop` は
同義ではありません。集約値のコピーと一時配列の索引でも環境の複製・解放を漏らさないでください。
非 Task の捕捉が1個だけで、整数・bool・f32/f64のビット幅がターゲットのポインター幅に収まる場合は、
環境欄をアドレスではなく値のビット列として使います。`immediate_capture` が生成・読出し・adapterで同じ条件を判定し、
`inttoptr`／`ptrtoint` と浮動小数点の bitcast だけで受け渡します。この欄を dereference／free してはいけません。
複製・解放は共有の恒等／無処理関数を使い、ゼロ値で環境欄が null になっても同じ値として扱います。
所有値・参照・複数捕捉・広幅値は従来の環境を使い、wasm32 の i64/f64 も従来経路です。
1個の直接格納から部分適用で複数捕捉へ進むときは値を復元して通常の環境へ移します。
記述子の4欄・公開 ABI・所有権／借用規則は変更しません。
通常の apply adapter は内部の末尾引数 `i1 borrow` で所有／一時借用を区別します。
所有モードは受け取った環境を消費し、次の段階へ所有値を移すか worker に渡して環境本体を解放します。
借用モードの完全適用は既存の読み取り専用解析を使い、解放不要な捕捉は通常 worker、所有捕捉は借用 worker に渡します。
消費的な捕捉・部分適用・特殊化予算切れは adapter 内で独立した環境を複製して所有モードへ進みます。
Task の一回消費 adapter にはこの引数を追加せず、Task ABI を維持します。
共有参照の loan は関数値・集約値のコピーを通して引き継ぎます。
排他参照を保存する再利用可能な環境は `Capture` 制約で拒否し、完全適用の一時的な段階だけ許します。
公開 ABI には環境ポインターを露出せず、必要なら全引数を受け取る bridge を生成します。

**継続の特殊化:** 型・所有権検査済みの通常の関数呼び出しを対象にし、CE の操作名を特別扱いしません。
関数引数が完全適用の callee、または他の非 escaping 引数・コレクション初期化にだけ使われることを、
相互再帰も含む固定点で確認します。結果が loan を運ぶ関数は保守的に除外します。
呼び先と捕捉数が分かる関数値には `(関数 ID, 既知の関数引数)` ごとの worker を生成します。
一時的な環境は entry block の alloca に置き、呼び出し終了後に捕捉した所有値を解放します。
callee の解析で引数を先に評価してはいけません。`|>` だけは元の規則どおり引数を先に評価します。

既知の callee が捕捉した所有値を読むだけなら、その prefix 引数の drop を行わない内部 worker を使います。
元の環境・所有者は呼び出し側が保持します。部分適用で所有値を置換する関数や、非 Copy 捕捉値の
move はこの経路から除外します。Copy 捕捉値を値として取り出すときの必要な複製は維持し、
借用 worker の prefix を一回使用の所有値と誤認して move／drop してはいけません。
普通の所有引数・ローカルは引き続き通常の drop を行います。
後続引数に可変参照・代入などがあれば、callee のスナップショットを保持する通常経路を選びます。
関数値そのものが外へ逃げる／コピーされる場合は従来のスナップショットを維持します。
呼び先が不明でも、place の callee と無害な引数なら呼び出し中だけ環境を借り、adapter が安全な経路を選びます。
配列／リストの未知の初期化関数も保持した環境を借り、生成終了後に一度だけ元の環境を解放します。

本体が引数をそのまま返すだけの関数は透過的に扱えますが、名前だけで `Delay` や `Return` を恒等関数とはみなしません。
元の AST 全体で参照が一箇所だけの所有ローカルは、消費する読み出し時に Copy でも領域を移せます。
その際は元スロットをゼロ化し、スコープ終了時の二重解放を防ぎます。複数使用・部分フィールド・
借用 worker の入力には適用しません。配列・リスト初期化も既知の callee なら、元の順序で
初期化関数の式を一回だけ評価し、各添字の呼び出しを直接行います。

追加 worker は順序付きキーとキューで重複排除し、最大 1,024 までです。上限後の呼び出しは通常経路へ戻します。
既にスタック環境を借りた worker 内でこの上限に達しても、通常 adapter に渡すのは独立したヒープ複製であり、
スタック環境を free してはいけません。関数記述子の4ポインター表現と公開 ABI は変更しません。
通常の型エラー・move エラーを最適化で消して受理することはなく、数値・範囲・確保サイズの検査も維持します。

**数値:** 各整数幅・符号、各浮動小数点形式は別の型です。`byte`／`ubyte` は `i8u`、`sbyte` は `i8` の別名です。
リテラルは型の文脈または接尾辞で決定し、変数を暗黙変換しません。
接尾辞のない整数リテラルは `GenericInteger` の推論変数で、関数末尾まで決まらなければ `i32`、浮動小数点リテラルは `f64` です。
浮動小数点・decimal・`bigint` を期待する位置の整数リテラルはその型のリテラルとして読みます。
binary リテラルを f64 に落としてから f128 に拡大するような二重丸めは禁止です。
decimal は BID の有限値／非正規化数／符号付きゼロ／無限大／NaN を保持します。
ソフトウェア演算は基数 2 または 10 の整数係数と指数を復号し、多倍長整数で計算して、
目的の精度へ一度だけ最近接・偶数丸めします。binary64 による近似代用は行いません。

`Display`／`Parse` は `Operation::Builtin` を持つ組み込みクラスで、通常の単相化したメソッドを
`BuiltinInstance` の Display／Parse 呼び出しへ下げます。標準 Option がないカスタム std 入力では、
Parse のシグネチャの解決失敗を使用時に報告し、無関係な解析を阻害しません。
`to_string` の独自 Display 呼び出しは、引数を借用して表示し通常どおり drop する型付き関数を生成します。
再帰検査もこの Display の依存辺を含めます。string の消費的な文字列化は所有権をそのまま返します。
utf8string の Display は UTF-16 へ変換し、消費的な文字列化では変換後に元の UTF-8 領域を解放します。

数値表示の入口は `tz_soft_format` に統一し、コンソール・Display・to_string が同じ形式を使います。
整数は100で割って2桁ずつ生成し、値が64-bitに収まるまでだけ u128 の除算を使い、それ以降は u64 に下げます。
桁対は32-bit算術で分解し、外部の係数表やホストの文字列化には依存しません。
狭幅整数の解析の cutoff 計算も u64 で行い、整数の範囲・符号・区切りの規則は維持します。
数値の load/store は対応する8/16/32/64/128-bit幅の固定サイズコピーを使い、アラインメント・別名関係を仮定しません。
元の little-endian バイト処理は C のフォールバックとして残し、生成 IR の対象は従来どおり little-endian です。
多倍長の作業領域を使う浮動小数点の表示・解析は非インラインの内部関数に分け、整数経路のスタックを小さく保ちます。
binary の正負ゼロの表示と、`inf`／`nan` の解析は多倍長処理に入る前に完了します。
数値表示の ASCII 出力は検証済みの内部専用変換で直接 UTF-16 に拡張します。
一般の UTF 変換は厳密な計数の結果から全 ASCII と証明できた場合だけ書き込みを単純化します。
非 ASCII の変換でも、計数で入力全体を検証した後の書き込みだけは内部の `validated` 引数を使い、重複した検査を省きます。
入力は同じ不変の借用領域で、計数・単独の well-formed 検査・修復は必ず未検証モードを使います。外部入力を無検査で受理する API ではありません。
UTF-16/UTF-8 の一致検査は境界内の64-bit比較を使い、辞書順は不一致位置の符号なし16-bit単位で決めます。
binary は既存 `tzrt_big` による Dragon4 の正確な境界区間から最短の桁を生成します。
2 の冪で狭くなる下側区間・最小 normal の例外・偶数 significand の閉区間を明示し、
同じ桁数の候補は近さと十進仮数の偶奇で選びます。decimal は既存 BID 復号を共有して末尾ゼロを正規化します。
第三者の実装・係数テーブル、ホストの浮動小数点や libc の変換関数には依存しません。

Parse の入口は UTF-16 の string を受け、4096 コード単位以下の ASCII だけを一時バッファに狭めます。
非 ASCII・孤立サロゲートは None で、UTF-8 変換や暗黙の置換はしません。
`tz_soft_parse` は従来どおり 4096 バイト以下の厳密な ASCII 文法を検査し、整数は対象幅の cutoff と最終桁で overflow を判定します。
十進整数の先頭に8桁以上の数字が続く場合、境界内の8バイトをまとめて検証・集約します。
基数100,000,000の cutoff と残余で範囲を確認してから蓄積し、区切り文字・非数字・末尾に達したら従来の1桁ループへ移ります。
失敗した8桁検査では入力を進めず、符号・先頭ゼロ・桁区切り・非十進数の規則を維持します。SIMD命令や追加の CPU 要件はありません。
float は十進係数・指数を正確な有理数として既存 `pack` に渡し、目的の形式へ一度だけ丸めます。
係数 4096 桁と形式ごとの指数範囲が `LIMBS` 内に収まるよう、極端な指数は拡大前に overflow／underflow と分類します。
失敗は 0（Option.None）で、成功時だけ出力スロットを初期化します。LLVM は成功分岐だけでスロットを読み、
標準 Option の Some／None の tag と共通 payload 領域を組み立てます。
表示用の一時バッファは entry alloca の 128 バイトで、ASCII の結果を `tz.string.from_ascii` で UTF-16 の所有値にします。

**文字型:** `Type::Char` は全ビットパターンが有効な LLVM `i16`、`Type::Utf8Char` は検査済み Unicode スカラーの `i32` です。
ソースの別々の literal variant を型検査後は既存の整数定数ノードで保持し、型で区別します。
比較は符号なし、match はそれぞれ `switch i16`／`switch i32` に下げます。整数 cast と公開 ABI は許可しません。
`runtime/character.ll` は Display/Parse と最大4バイトの UTF-8 出力を共有し、char のサロゲート出力だけを拒否します。
必要時だけ既存の console/string runtime と連結し、WASM import は増やしません。

**文字列:** `Type::String` と `Type::Utf8String` は別の非 Copy 型です。
構文・型付き IR の `StringLiteral::Utf16(Vec<u16>)` は Rust の String を経由せず孤立サロゲートを保持し、
`StringLiteral::Utf8(String)` は従来の妥当な UTF-8 を保持します。match の定数キーも符号単位を失わず比較します。
LLVM は `%tz.string`／`%tz.utf8string` の独立した `{ ptr, i64 }` 記述子を使い、
長さはそれぞれ UTF-16 コード単位数／UTF-8 バイト数です。
`std/String.tz`／`std/Utf8String.tz` の `length` は共有借用の `.length` を使う通常の std 関数です。
同じ記述子の長さを直接取り出すため、ランタイムの走査・複製・変換は不要です。
UTF-16 の定数は `[N x i16]` として出力し、ホストの endian に依存したバイト列を埋め込みません。
UTF-16 の確保は長さが 2^53 - 1 以下であることを検査してから2倍し、連結にも上限検査を適用します。
索引・複製・連結は型ごとの直接の LLVM 操作で、符号化を実行時に判別する汎用 dispatch はありません。
string の大小比較は符号なし i16 の辞書順で、暗黙の Unicode 正規化はありません。
utf8string は符号なし i8 の辞書順です。モジュールの compare と比較演算子は同じ型別 helper を使います。
検索・構築・切り出しは通常の std ソースと Vec の単相化を使い、必要な容量を先に確保します。
`llvm_bulk.rs` の型付き builtin が string/i16u 配列、utf8string/ubyte 配列の所有 descriptor を移送します。
UTF-8 検証は既存 decoder の失敗を値で返す内部版を共有し、from_bytes は解放して None、厳密な復号はトラップします。
未使用の std 関数は型検査だけを行い、利用者側から呼ばれた関数だけを特殊化するため、未使用 std は特殊化上限を消費しません。
UTF-8 への変換・コンソール出力は孤立サロゲートでトラップし、置換は `String.to_well_formed` だけが行います。
所有権・フレームからのヒープ移送・捕捉の複製・drop は両型に同じ規則を適用します。

**タスク:** `Type::Task(T)` は非 Copy の cold な一回実行の計算です。
型検査では `task` を引数なしの Lambda として扱い、捕捉と結果には `Capture` ではなく `Send` を課します。
参照を含むデータと借用のある関数環境を拒否し、タスク内部の借用は外へ出させません。
所有権検査がタスク生成時の環境を検証するため、lift された `is_task` worker の捕捉引数は
閉じた所有値として扱えます。通常の関数の関数値引数には同じ仮定を置きません。
`Task<T>` は loan を運ばないという不変条件を、抽象本体と具体化後の両方で維持します。
通常の再利用可能な環境へのタスク捕捉は拒否します。

LLVM では `%tz.closure` の apply／environment／drop を再利用しますが、clone ポインタは null で、
タスク用環境の clone 関数は生成しません。タスクを含む一時的な完全適用用環境も clone 不可です。
`let!`／`return!` と `Task.run` は同じ一回消費の lowering を使います。
`Task.parallel` 自体は配列を所有する cold タスクを作り、実行時だけ結果用の連続領域を確保します。
各入力スロットを一回ずつ消費する、結果の LLVM 型ごとの callback を決定的に重複除去して生成します。
callback の C 境界は `(void *context, uint64_t index)` のみで、集約値の ABI を C に露出しません。
全 callback が完了してから入力の配列領域を解放し、初期化済みの結果配列を返します。
グループの context を含め、反復で使う alloca は関数 entry に置きます。

ネイティブランタイムは pthread_once により `min(sysconf(_SC_NPROCESSORS_ONLN), 32) - 1` 本の常駐 worker を遅延起動します。
CPU 数取得失敗時は追加 worker なしです。0件・1件はプール起動なしで呼び出し元が処理します。
mutex が FIFO の active group list と next index の割当を保護し、callback 中には保持しません。caller は自分のグループを進めます。
remaining は callback の結果 store 後に acq-rel RMW で減らし、caller が acquire で0を確認してから mutex 下で group を外します。
最終 decrement 後の worker は group を読まず、共有の完了条件変数だけを通知するため、stack group の寿命を超えてアクセスしません。
Parallel の完全適用は専用 `TypedExprKind::Parallel` とし、通常の Call へ消去する前に callback と結果の所有環境を検査します。
借用結果の検査は curried callback の1引数／2引数適用段階を区別します。mapper-first API は入力型を先に推論しますが、実行時の引数順は変えません。
`llvm_parallel.rs` は4096要素目標・最大1024チャンクの分割式と `void(ptr, i64)` callback を生成し、Task 配列を作らず同じ runtime を呼びます。
証明済みの非消費 callback は既存の特殊化を直接呼び、その他はチャンクごとの保持 snapshot と full application ごとの clone を使います。
入力・identity は必要箇所で `clone_value` を使い、結果領域は worker 起動前に確保します。reduce の最終結合はチャンク index 順です。
idle worker は条件変数で待ち、atexit shutdown が停止・wake・join を行います。pthread・同期・atexit の失敗は stderr と abort で明示します。
WASM は同じ callback を添字順に呼び、ホストのスレッド機能を要求しません。
独立した仕事の実行順は未規定ですが、各仕事の内部の評価順序と結果配列の順序は保持します。

ネイティブ並列 IR の外部シンボルは `tsuzuri_task_parallel` とし、`tz_` の公開 ABI と衝突させません。
driver は必要なときだけ同梱 C をコンパイルし、実行ファイルには `-pthread` 付きでリンク、
オブジェクトには relocatable link で組み込みます。ランタイム入口は weak/hidden で、
複数の生成オブジェクトを同時リンクしても一つに統合します。
macOS の relocatable link は `-keep_private_externs` を使い、中間リンクで weak/hidden 入口が局所化されるのを防ぎます。
単一シンボルと、二 object の同時実行で合計 worker 数が上限内であることをテストします。
生のネイティブ LLVM IR を直接リンクする利用者は `src/runtime/task.c` と `-pthread` を追加します。
LLVM IR／ヘッダーの出力と Cargo ビルドには、引き続き LLVM・pthread ヘッダーを要求しません。

**所有権:** 型検査済みのローカル ID とフィールド経路に対して move と loan を検査します。
引数などの一時的な loan も、後続オペランドの評価が終わるまで保持します。
再借用は元の loan を親として追跡し、子の生存中に元の排他参照を使用・移動できません。
参照を結果・外側の束縛へ移すときは、参照先の所有者が生存していることを検証します。
共有借用フィールドを許し、格納型の走査はrecord/union/collectionを通して参照・排他参照を検査します。借用省略は入力loanの交差を保持します。
`TypeExprKind::Regions`と宣言のregionリストを`regions.rs`で検査し、返却契約をCheckedFunction.region_sourcesへ保持します。
契約は結果のregion slotごとに、借用元になれる（引数添字、引数のslot）の組を持ちます（`RegionSources`）。多region recordは`CheckedRecord.field_regions`にfieldごとのregionの`RegionMask`（`u16`、最大16）を持ちます。
所有権検査の`Value`は多region recordの値だけslotごとのloan集合を持ち、recordリテラル・更新・fieldの読み出し・不変の束縛・`if`／`match`の合流・直接の完全適用でslotを保ちます。
その他の経路はslotを捨てて全loanを保持します。所有権検査は返却loanのexternal rootが契約に含まれることを確認し、直接の完全適用だけ指定入力のloanを戻り値へ伝えます。
名前付き関数の引数のregion量化関数型（`TypeExprKind::Quantified`）は、関数ごとの`CallbackContract`になります。
本体でその引数を完全適用した結果は契約の入力のloanだけを持ち、呼び出し側は渡した関数（名前付き関数の契約、ラムダの本体、同じ契約の引数）が契約を守ることを検査します。
その関数は直接の完全適用でだけ使え（`regions::validate_contract_calls`）、契約が関数値へ漏れません。
通常のType/LLVMからはregionと量化を消去し、量化のない関数値・部分適用は全入力の保守的追跡を維持します。
参照型の構文解析は別helperへ分離し、既存の深さ128でパーサーのスタック使用量を維持します。
レコード／配列／リストの Copy は構造的に決まり、文字列を含む値は所有権を移動します。

**配列:** `Type::Array` は要素型だけを保持し、LLVM では `%tz.array = { ptr, i64 }` に下げます。
`new [T](length, initializer)` と `new [...]` は所有ヒープ領域を確保し、束縛したリテラルはフレームの領域を使います
（後述の **記憶域**）。どちらも生成後は長さ・要素を変更しません。
初期化関数の式は一度だけ評価し、各添字で関数値のスナップショットを適用します。
確保前に負の長さ・要素サイズとの積のオーバーフローを検査します。
ヒープに確保する空配列とサイズ 0 の要素でも確保サイズを 1 バイト以上にし、`malloc(0)` の挙動には依存しません。
`new` なしの空リテラルは領域を持たない `{ null, 0 }` で、その解放は何もしません。
配列の Copy と捕捉環境の複製ではバッファを独立に複製し、要素の clone／drop も反復します。
要素型がスカラーでもバッファの解放が必要なため、すべての配列で `needs_drop` は true です。
長さの取得・索引・借用だけで配列全体を複製せず、要素取得に必要な複製と一時所有値の解放だけを行います。
単相化・型クラスのインスタンス選択に配列長は関与しません。再帰 union の要素は上記の反復走査へ登録します。

**連結リスト:** `Type::List` は `[|T|]` に対応し、LLVM では `%tz.list = { ptr, i64 }`
（先頭・長さ）と `{ ptr, T }`（次のノード・要素）に下げます。
`new [|...|]` と `new [|T|](length, initializer)` は評価順にノードをヒープに確保し、
束縛した `[|...|]` はフレームの連続したノード配列を連結します。生成後は変更しません。
空リストの先頭は null でノードの確保は不要です。`.length` は O(1)、索引は範囲検査後にリンクを辿ります。
構造的 Copy・捕捉環境の複製ではノードと要素を独立に複製し、すべてのリストで `needs_drop` は true です。
clone／drop／索引は反復で処理します。drop ではノードを解放する前に次のポインタを読みます。
生成用の一時スロットも entry に置き、長いリストや末尾再帰でスタックを蓄積しません。
配列と同じ多相化・所有権・借用追跡・ABI 制限を適用します。

**記憶域:** `new [...]`／`new [|...|]` は構文 AST・型付き IR の `NewLiteral` で包み、型と所有権は内側のリテラルと同じです。
スタックかヒープかは型ではなく LLVM emitter が生成位置で決め、`Type`・型クラス・単相化・ABI には現れません。
`let` の初期化式・match の対象・for の列挙元・索引／長さ／文字列の比較と連結の被演算子にあるリテラルの木
（if／match／ブロックの結果と、レコード・タプル・コレクションのリテラル要素を通じたもの）を `frame_value` で下げます。
配列は `[N x T]`、リストは `[N x { ptr, T }]` の entry alloca、文字列は private 定数を直接指し、空のものは `{ null, 0 }` です。
各値にはその位置に置いたフレーム領域の候補（`Frame`）を静的に対応付けます。

フレームのポインターはその値を受け取った束縛（または一時値）の中の静的な位置と、呼び出し中だけ借用 worker に
貸す引数にしか存在しない、が不変条件です。
場所からの消費的な読み出し（move）はすべて `relocate` を通し、戻り値・実引数・捕捉・代入・別の束縛・要素・
末尾ループのバックエッジのどこへ移る値もヒープだけを指します。Copy の値の複製は従来どおり `clone_value` がヒープに作ります。
例外は値を読んだ直後に破棄する消費者です。文字列連結の被演算子と、借用 worker に渡す一時的な環境の捕捉
（`capture_values`）はフレームのまま受け取り、連結後・呼び出し後の解放でフレームを考慮します。
借用 worker は prefix を move・drop しないことが特殊化の条件なので、呼び出し中もフレームの寿命内です。
`&mut` の借用前には場所の値をヒープへ移してから貸すため、借り手の置換・drop はフレームを解放しません。
共有借用・索引・長さ・for・パターンの読み取り専用別名は移動を伴わず、所有権検査が寿命を束縛のスコープ内に限定します。
値が候補のフレームをまだ使っているかは、実行時にデータ／先頭ポインターと長さの両方を比較して判定します。
これにより部分 move 後のゼロ、`let mut` への再代入、分岐の合流でも静的な追跡なしに正しく扱え、
長さの比較が移動済みのゼロと WASM のアドレス 0 の alloca も区別します。
フレームの場合の drop は要素の所有値だけを解放し、ヒープへの移動は要素をビット単位で移して入れ子の候補を再帰的に移します。
配列・リストの要素は不変で取り出して move できないため、親がフレームのままなら入れ子の要素も元のフレームを指します。
一つのリテラルの木のフレーム領域が保守的な見積もりで 64 KiB（`MAX_VALUE_BYTES`）を超える場合は、木全体をヒープで生成します。
束縛しない戻り値・実引数の位置のリテラルは移動を省いて直接ヒープに作ります。
関数値の環境とタスクは対象外で、既存の特殊化（既知の非 escaping 呼び出しのスタック環境）を使います。

**コレクションの不変性:** 配列・リストの要素への代入・可変借用を拒否します。
要素から入れ子・共有参照を辿って可変参照に到達できる型も `validate_size` で拒否し、
単相化後の全型にも再適用します。関数シグネチャ中の可変参照は格納状態ではないので対象外で、
排他参照の捕捉は既存の Capture 検査で拒否します。共有借用の要素の寿命は通常の loan 追跡で保持します。
可変束縛や `&mut` によるコレクション全体の置換は引き続き許可します。

**LLVM:** 整数の算術に `nsw`／`nuw` を付けません。除算／剰余はゼロと符号付き MIN/-1 を検査し、
シフト数を幅ごとにマスクし、浮動小数点→整数には飽和変換を使います。
`@checked` の中の整数の `+`・`-`・`*`・`**`・単項 `-` だけは `Int.checked_*` と同じ `checked_integer_arithmetic` で検査し、
overflow で `OverflowException` を送出します。整数の `**` は `Int.wrapping_pow` と同じ二乗と乗算の反復（符号付きの負の指数は `NumericRuntime` のトラップ）、
`f32`／`f64` の `**` は `Math.pow` と同じ `tz_math_pow_f*` です。
例外は `i32` の code で、同じ関数本体の最も内側の `try` の handler へ、途中のスコープと一時値を解放して分岐します（`try_targets`）。
関数・ラムダの境界は越えず、囲む `try` がなければ `TrapKind::Overflow` でトラップします。`finally` は handler から出る例外も含む全経路で一度実行します。
i8〜i64／i8u〜i64u から f32／f64 へは `sitofp`／`uitofp` を使い、
逆方向は `llvm.fptosi.sat`／`llvm.fptoui.sat` にして poison や範囲外の未定義動作を避けます。
f32／f64 間は `fpext`／`fptrunc`。i128、f16／f128、decimal を含む変換は正確な
ソフトウェア経路を維持し、f64 を経由して丸めを二重にしません。
intrinsic 宣言は builtin と通常の式で共有し、重複なく決定的な順序で出力します。
負の添字を含め、配列・リストの範囲検査を要素の GEP／load やリンクの走査前に行います。
浮動小数点に fast-math フラグを付けません。
借用可能な値の領域は entry に置きます。所有値を move すると移動元をゼロにし、
スコープ終了時に残った文字列・配列・リスト・捕捉環境を解放します。レコードの部分 move も同じ仕組みです。
フレームの領域を指しうるスロットの解放・代入前の解放は、前述のアドレス判定でフレームを free しません。

**末尾再帰:** 直接の自己末尾呼び出しは引数評価の後で loop に分岐し、
全引数を phi のバックエッジで同時更新します。
native では引数の整数 `+`／`-` を、オペランドを元の左から右の順で SSA 値へ読み出した後、
演算命令だけを全引数の評価後・drop 前の更新部分へ配置します。
これによりカウンター更新を本体の状態計算と分離し、LLVM が while と同等の命令選択を行いやすくします。
延期するのは副作用もトラップもない幅固定の折り返し加減算だけです。
オペランドの load・呼び出し・トラップ・一時所有値の解放は動かさず、後続引数が可変ローカルを変更しても
先に得た SSA 値を使います。終了条件・負数の挙動・overflow フラグ・浮動小数点の順序は変更しません。
既知の二引数関数で本体が引数同士の一つの演算だけなら同じ経路を使い、
`Add.add`／`Sub.sub` と演算子の違いを作りません。検査や他の処理を持つ関数は通常の呼び出しを維持します。
WASM ではスタック機械向けの従来の引数生成順を保ちます。
両方の順序で同じ意味を検査し、native で速かった命令配置を別の実行基盤へ無条件に適用しません。
所有するローカル・配列アクセス・数値演算の一時 alloca とスタック上のリテラルの領域は必ず entry に置きます。
これをループ内に置くと、ソースでは末尾再帰でもスタックを消費し続けるためです。
反復ごとに同じリテラルの領域を再初期化しても、前の反復の値はバックエッジ前に解放済みか、ヒープへ移動済みです。
反復前に未移動の所有値を解放します。借用を含む引数がある場合はフレーム再利用を行いません。

**WASM:** bulk-memory を有効化し、集約値のコピーにホストの memcpy を要求しません。
LLVM の閉形式ループ最適化は i64 プログラムにも i128 の乗算／除算を導入します。
同梱の乗算は i64／32-bit limb、除算は固定シフトと減算で実装し、
その補助自身が wide multiply/divide libcall に再変換される循環を避けます。
LLVM 23 は limb の乗算式を再び i128 乗算として認識するため、乗算補助だけは
`noinline optnone` で InstCombine から保護します。可変シフトの limb 補助も同様に保護します。
アプリケーション関数の最適化は維持します。
weak/hidden な補助はオブジェクト間で重複を許容し、未使用なら LLD が削除します。
WASM ヒープはアドレス順の空き領域リストを使い、分割・隣接領域の結合を行います。
memory.grow の失敗・上限（既定 16 MiB）超過ではトラップし、成功した形の無効ポインターを返しません。
上限と main stack は `BuildOptions`／`TestOptions` の `wasm_max_memory`・`wasm_stack_size`（`--wasm-max-memory`・`--wasm-stack-size`、`None` は既定値）です。
main は project の読み込み後、未指定のものを root package の `[wasm]`（`Project::wasm`、`BuildOptions::with_manifest_wasm`）で補い、適用後にもう一度検証します。
`driver::wasm_memory_limits` が target ごとの範囲（wasm32 は 4 GiB − 64 KiB、wasm64 は 16 GiB）を一か所で検査して実効値を返し、build と test runner はその値だけから wasm-ld の `-z stack-size=` と `--max-memory=` を作ります。
ヒープ側は `heap-wasm.ll` を変えず、`llvm::with_wasm_heap_limit` が生成後の IR の `%fits` 2 行と `%within` 1 行だけを行全体の一致で置換します。
利用者の文字列定数は `c"..."` の行の途中にあるため変わらず、既定値では何もしないので既定の IR と `.wasm` は不変です。
上限が 2 GiB 以下なら `i32` の終端 `%begin + %needed` は折り返りません。超えるときは `%within` を `%room = sub i32 LIMIT, %begin` との比較の 2 行に置き換えます（`%begin` は上限を超えません）。
上限を 4 GiB − 64 KiB までにするのは `%ceil = add i32 %end, 65535` が折り返らないためです。JS の同梱ホストは pointer を `>>> 0` で受け取ります。
wasm64 は `heap-wasm64.ll`（header は i64 の capacity・next、`memory.size`/`grow` は i64）を使い、同じ 3 行の置換で上限を入れます。
`Instrumentation::memory64` は host ABI の memory 範囲検査、scalar capture の幅、DWARF の pointer 幅を 64-bit にし、clang は `--target=wasm64-unknown-unknown`、wasm-ld は `-mwasm64` です。threads は wasm32 だけです。
stack は尽きると 0 の下へ折り返ります。wasm32 の 2 GiB 超と threads（worker の stack は heap の block）ではその先が memory 内になり得るため、`llvm::with_stack_checks` が全関数の entry block の static alloca の後へ `@tz.stack.check` を入れます。
alloca を呼び出しの前へ集めるのは、inline 展開が entry block を分けたときに動的 alloca（ループで stack が増え続け、SROA も効かない）にならないためです。
検査は `llvm.frameaddress(0)`（prologue 後の stack pointer。副作用なし）を `[__stack_low + 4096, __stack_high]` と比べ、外れたら `llvm.trap` です。`llvm.stacksave` は副作用を持ち LICM・vectorize を妨げます。
検査は trap-info の計測より前に入れるため、`@tz.stack.check` も context を受け取り、溢れを `stack overflow` として報告します。計測が後から足す reporter と `tsuzuri_trap_site` は検査を持たず、stack pointer が範囲外のままでも呼べます。
threads では wasm global の `tsuzuri_stack_base`・`tsuzuri_stack_top`（instance ごと、`!invariant.load`）が 0 なら main、そうでなければ worker の範囲です。余白 4096 は検査のない `task-wasm-threads.c` と `wasm.ll` の frame の合計（O0 で 640 bytes）を含みます。
threads は import の max も同じ値になり、`--emit object` はヒープの定数だけを持ちます。cache key は options と置換後の IR で上限を含みます。
`tests/wasm_memory.mjs` は O0/O3 で既定出力の不変、2 GiB 超の room 形式と境界、stack 検査、manifest、診断を、`tests/wasm_threads.mjs` は worker の溢れが隣の block を書かないことを、`tests/wasm64.mjs`（Node.js 24）は wasm64 の上限・境界・4 GiB 超の host buffer・test を検証します。

**ホスト:** Tsuzuri 関数には外部関数をインポートしません。
コンソールの putchar は生成したエントリー・ラッパーだけが持ちます。数値の printf 依存はありません。
公開 ABI は従来の scalar に加え、借用 i64/f64/ubyte 配列と両文字列型の入力、所有バッファ結果、scalar-only record を扱います。
`abi.rs` が型・方向・record field 名を共通分類し、`llvm_abi.rs` が header と wrapper を生成します。
buffer 入力は pointer/length から直接 descriptor を構築し、shared array は SSA 値、文字列は stack descriptor の参照を渡します。
string は i16 のコード単位、utf8string は i8 のバイトです。UTF-8 検証は既存の失敗を値で返す decoder を使い、入力を確保・変換しません。
record は専用 ABI struct と内部型の間で再帰的に変換し、bool/狭い整数は32-bit。結果は out pointer に書き、buffer の所有権だけをホストへ渡します。
allocator は拡張 ABI 使用時だけ weak な tsuzuri_alloc/free を公開し、native の malloc/free または既存 WASM heap を使います。
POSIX native の allocator は共通のフック表から境界 runtime を検出し、確保と解放を追跡経路へ送ります。これは拡張 ABI を使う既定 native IR の変更ですが、公開シグネチャは変わりません。WASM と Windows の既定 allocator は従来のままです。
128-bit 値、ソフトウェア浮動小数点、任意の所有入力、借用返却、関数環境の ABI は公開しません。
import は利用者の extern だけから生じ、link 名・ハンドル・コールバックを使わないプログラムの IR・header・WASM import は変わりません。
リンク入力は native の実行ファイルだけで有効です。wasm-ld の `--export-table` は callback の wrapper があるときだけ渡し、捕捉のある関数値は ABI に渡しません。
GUI、入力、ネットワーク、非同期 I/O／イベントループはホストの境界で扱います。ファイル・環境・時刻・乱数・子プロセスは std の OS API が扱い、利用者の extern や公開 ABI は増やしません。

**出力:** 入力全体の検査後、出力先と同じファイルシステムの専用ディレクトリでビルドします。
全ツールが成功した後にだけ rename で成果物を公開します。
出力保護はルートだけでなく読み込んだ全ソースに適用し、Unix ではソースへのハードリンクも拒否します。
他のプラットフォームでも atomic replacement が
別のハードリンクの内容を書き換えることはありません。
出力ファイルを先に truncate しないでください。

## 開発と検証

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
node tests/os.mjs target/release/tsuzuri
node tests/examples.mjs target/release/tsuzuri
node tests/features.mjs target/release/tsuzuri
node tests/wasm_memory.mjs target/release/tsuzuri
npx --yes --package=node@24 node tests/wasm64.mjs target/release/tsuzuri
```

Rust のテストは LLVM なしで走ります。字句・型・失敗例・レイアウト・IR の不変条件・
6,500 パターンの決定的なソース変異を検査します。
`tests/diagnostics.rs` と CLI E2E は複数ファイル・宣言境界・括弧内の行頭 let・壊れたシグネチャ／認識器の抑制、
型／所有権の関数内の回復・スコープ復元・壊れた関数の借用返却値からの二次エラー抑制、50 件を超える正確な省略数と 1000 件の収集上限、
既定スタックでの深い回復、JSON lines・出力保護を検査します。
所有権では正常な move／共有借用／排他借用に加え、move 後の使用、部分 move、
分岐・短絡評価、再借用、寿命切れ、オペランド評価中の参照先の無効化を検査します。
Node の E2E は本物の Clang／LLD、ネイティブ C ホスト、WebAssembly エンジンを使い、
JavaScript の BigInt／Number の参照結果と照合します。
最適化無効／有効の両方を実行し、百万回の末尾再帰、トラップ、
キャリーを含む全幅 128-bit 補助を検査します。外部ツール不足をスキップして成功扱いにはしません。
例の検証には HTTP 経由の WASM 読み込み、Python デスクトップ・ホストの headless 実行も含みます。
複数ファイルでは名前の分離、修飾された高階関数・レコード、相互再帰、
`Main.tz` の選択、ファイルごとの診断、依存ソースの出力保護を検査します。
`tests/polymorphism.rs` は抽象本体・型クラス制約・関数制約・インスタンス重複・特殊化・型の増大を検査します。
`tests/e2e.mjs` の多相 fixture は C／WASM で整数・浮動小数点・独自レコードの演算、
高階関数、型クラスのメソッド値、型によるモジュール関数の選択・部分適用、所有文字列、評価順序、百万回の末尾再帰を実行します。
`tests/primitives.mjs` は decimal の結果を Python の IEEE 用 decimal コンテキストと照合し、
ネイティブの確保／解放を追跡して解放漏れ・二重解放を検出します。
`tests/numeric_casts.mjs` は整数の全幅・符号と f32／f64 の高速変換を、BigInt の直接丸め、
飽和の参照結果、および f128 を経由する正確なソフトウェア実装と照合します。
NaN、無限大、符号付きゼロ、非正規化数、丸めの中点と二重丸めが起きる値、
飽和境界を native／WASM の `-O0`／`-O3` と native CPU 指定で検証します。
`tests/display_parse.mjs` は全 f16 ビット列、f32／f64 各 10,000・f128 2,000 の決定的なランダム列と境界、
decimal・整数全幅を、Python Fraction の区間内の整数仮数探索と Decimal の独立参照に照合します。
最短表示、非 NaN のビット往復、NaN の分類、decimal の数値と符号付きゼロ、解析の失敗・資源上限を
native／WASM の `-O0`／`-O3` で検査します。raw runtime harness はテスト専用で公開 ABI を増やしません。
独自インスタンスの解放・生 UTF-8・parse-only／to_string-only のリンク・コンソールとの形式一致も検査します。
`tests/strings.rs`／`tests/strings.mjs` は UTF-16 の型・サロゲート・符号化変換と従来の UTF-8 動作を検査します。
Node の String を参照に、コード単位数・索引・比較を native／WASM の `-O0`／`-O3` で照合します。
同じ runner が `tests/strings_runtime.c` の独立した整数演算の参照で全 Unicode スカラーを小分けに往復し、
不正 UTF-8・孤立サロゲートの変換を明示的なトラップとして検証します。
巨大な領域を確保しない記録用 allocator により、2^53 - 1 の長さ上限とコード単位当たり2バイトも照合します。
WASM では累積の確保量がメモリ上限を超える反復を実行し、空き領域の再利用を確認します。
カリー化では `tests/currying.rs` が型・捕捉・寿命を検査し、
`tests/fixtures/currying` を `tests/primitives.mjs` から C／WASM の両方で実行します。
匿名関数、途中段階の副作用、関数を含む集約値、入れ子の所有文字列、40,000回の
環境作成・複製・解放を追跡し、各公開関数の呼び出し後にネイティブの未解放バイトが 0 であることを確認します。
配列は `tests/arrays.rs` と `tests/fixtures/arrays` で型から長さが独立していること、
実行時生成、異なる長さの入れ子、空配列、不変性、借用の寿命を検査します。
`tests/primitives.mjs` では 1024 要素を超える生成、確保サイズのオーバーフロー、
所有要素・関数要素の複製と解放、100,000 回の末尾再帰、ネイティブ／WASM のトラップを確認します。
連結リストは `tests/lists.rs` と `tests/fixtures/lists` で同じ範囲に加え、区切りと演算子の共存、
型クラス、配列との混在、可変参照による不変性の迂回を検査します。
`tests/primitives.mjs` は連結ノードの確保数、100,000 ノードの反復解放、100,000 回の末尾再帰、
複製・所有要素・捕捉環境の解放、WASM のヒープ再利用とメモリ上限も検査します。
記憶域は `tests/storage.rs` が `new` の構文、束縛・一時値のリテラルの entry alloca、`new`・移動・`&mut` の
ヒープ確保の位置、64 KiB の上限の前後、末尾ループの entry 配置を IR で検査します。
`tests/fixtures/storage` は `tests/primitives.mjs` から native／WASM の `-O0`／`-O3` で実行し、
ネイティブでは各呼び出しのヒープ確保回数（束縛したリテラルは 0、`new`・スコープ外への移動・Copy の複製は各 1 以上）と
未解放バイト 0 を照合します。入れ子・レコード・文字列・リストの移動、部分 move、match の束縛、分岐の合流、
ループと 100,000 回の末尾再帰での移動、`&mut` の置換、連結、捕捉、128-bit・サイズ 0 の要素も含みます。

数値ランタイムの変更時は `src/runtime/numeric.c` を編集し、
`python3 src/runtime/generate.py` で `numeric.ll` を再生成します。
続けて`python3 src/runtime/generate_math.py`でmath.llも再生成し、metadata／attribute IDをnumeric.llの最大値より後へ割り当てます。
Mathを使う場合だけnumeric.llの後にmath.llを連結し、重複するLLVM intrinsic宣言を除きます。
trap／ループ用の生成metadataは両ランタイムの最大IDより後へ予約します。
math生成器は必要なmusl 1.2.5ソースを個別にコンパイルし、llvm-linkで結合します。
検証済み環境はApple Clang 21とllvm-link 21です。`TSUZURI_CLANG`／`TSUZURI_LLVM_LINK`で指定します。
保存前に同じClangでIRをコンパイル検証し、未解決外部関数・FMA・fast-math・拡張精度を拒否します。
Cargoビルド時のClang／llvm-link依存は追加しません。upstreamのライセンスと取得元hashはruntime/muslに同梱します。

Floatのsqrt／丸めはf32/f64で型付きintrinsic・明示演算、他の形式では既存の整数数値ランタイムを使います。
明示Math.fmaだけをAArch64 nativeのf32/f64でllvm.fmaへ下げ、命令を保証できないnative targetとWASM、他のFloat形式ではtz_soft_fmaを使います。
通常のfmul/faddにcontract/fast-mathは付けず、libmのfma importも作りません。builtin専用Globalsにもtargetのwasm設定を引き継ぎます。
soft FMAは係数の積と加数を符号付き多倍長整数として合成し、既存packで一度だけ丸めます。指数差が3p+8桁を超える非ゼロの小項は、丸めtieへの方向だけを保つsticky digitへ縮約します。
これにより既存の固定LIMBS領域を拡大せず、f128/decimal128の極端な指数差も扱います。特殊値を先に処理し、decimalは十進のまま計算します。
Arrayのpairwise/Neumaier/dot/dot_fmaは通常のstd関数です。pairwiseは一回の所有コピーをArray.setで更新し、各段の隣接ペア木を維持します。既存sumと通常dotは左順・別丸めのままです。
soft sqrtは目的の量子指数を求め、整数二乗比較で係数を探索し、二つの丸め候補の中点の二乗と比較して一度だけ丸めます。
decimalからbinaryへの変換は行いません。min/maxはNaN伝播と符号付き0、clampは順序検査を保ちます。
Elementaryはf32/f64だけです。muslの通常f64 atan2が1 ulpを超えたため、有限の通常比はdouble-double範囲縮小とdegree31級数へ置き換えました。
|reduced|は1/8以下、係数は256-bitからhigh/lowに分け、特殊値と極端な比はupstream処理を維持します。
速度向上の主張はしません。`tests/math.mjs`はFMAを含むBigInt参照の基本演算759675件と256-bit mpmath参照の超越関数26376件をnative/WASM O0/O3で照合します。
f16の単項基本演算は全65536パターン、広幅形式は各1000乱数を含み、非NaNのbit一致と実行時heap非確保も検査します。
featuresのfma_reductionsは長さ0..17と1025までの境界・奇数長で872ケースと2トラップを検証し、Cのfma_runtimeはホストfma/fmafと20513組をASan/UBSanでも照合します。

生成時だけ Clang を使い、通常の Cargo ビルド・型検査・LLVM IR 出力には LLVM のインストールを要求しません。
生成器はホストの target triple／データレイアウト／CPU 属性と新しい IR 限定の属性を除き、
対応する little-endian ネイティブ／wasm32 で同じ整数アルゴリズムを使います。
生成済み IR は直接手編集せず、C の変更と一緒に更新してください。
`numeric.ll` を含む `include_str!` 対象のランタイム IR はすべて Git で追跡します（`*.ll` の ignore には
`.gitignore` の例外で対応）。新しいランタイム `.ll` を埋め込むときは例外行を追加し、
`sh scripts/check-runtime-includes.sh` で追跡漏れがないことを確認します。
再生成の検証は `python3 src/runtime/generate.py` の後に `git diff --exit-code -- src/runtime/numeric.ll`
が通ることです。同梱の `numeric.ll` は Apple clang 21 で生成しており、同じ版では byte-identical になります。
LLVM 23 系の Clang はサイズ引数のない `llvm.lifetime.*` など LLVM 17 が読めない IR を出すため、
再生成には LLVM 17〜21 系の Clang を `TSUZURI_CLANG` で指定し、差分は手で取り込まないでください。

タスクは `tests/tasks.rs` が構文・単相化・move・捕捉された借用・決定的 IR を検査します。
`tests/tasks.mjs` は native／WASM の `-O0`／`-O3` で逐次 bind、結果順序、入れ子の並列処理、
所有文字列・コレクション・関数・タスクの結果、数値境界、trap を確認します。
ネイティブのヒープ追跡と WASM 上限を超える累積確保で、未実行タスクを含む解放も確認します。
`tests/task_runtime.c` は capability 検出と pthread 呼び出しを計測可能な境界に差し替え、
条件変数による実際の同時実行、共有枠の上限、全 join、逐次 fallback、作成／join 失敗の診断を検査します。
時間の速さを合否条件にせず、通常の関数呼び出し後には実行中の worker を残しません。

`tests/computations.rs` は拡張子ごとの宣言制約、複数型クラス、ビルダーの各構文、
単相化、捕捉、寿命、未実装操作、展開深さ、未使用ビルダーの検査を確認します。
`tests/computations.mjs` は native／WASM の `-O0`／`-O3` で独自の短絡・複数 yield・
入れ子の反復・Delay／Run の有無・評価順序・数値境界・trap・タスクとの合成を実行します。
ネイティブでは全呼び出し後の未解放バイトを 0 と照合し、WASM では既定の 16 MiB を超える累積確保の
反復を行います。注釈なしの配列 bind と手書きの操作呼び出しの確保量も一致させます。
`.tt`／`.tc` の診断先・出力保護と、決定的な IR／WASM も確認します。
`tests/call_specialization.rs` と `tests/fixtures/computations/Optimization.tz` は既知・動的・
逃げる関数値、可変引数の評価順序、所有する捕捉値・返却値、相互再帰、特殊化上限の通常経路を検査します。
コンピュテーションの実行テストには 520 個の異なる継続で予算を超える例と、
`-O3` の 262,144 回のコールバックを含め、native／WASM で解放・スタック・結果を確認します。
`benchmarks/run-computations.mjs` は最適化後 IR の確保数と手書き Tsuzuri／C++ との時間を別々に測定し、
共有 CI では `--quick` による正しさの検査だけを行います。
Clang の AddressSanitizer が使える環境では
`TSUZURI_ASAN=1 ASAN_OPTIONS=detect_stack_use_after_return=1 node tests/computations.mjs target/release/tsuzuri`
により、生成したネイティブ IR のヒープ・スタックへのアクセスも検査できます。

`tests/control.rs` と `tests/control.mjs` は新構文、単相化、再帰の指定漏れ、move／loan の反復、
OR の束縛とガード、認識器の順序・解放、タプル、宣言・パターンの深さを検査します。
native／WASM の `-O0`／`-O3` で、整数端点・正負の step・ゼロ step・空列挙・NaN・符号付きゼロ・
software 数値・密な表の範囲外・百万回の match 末尾再帰を実行します。
ネイティブの確保追跡と WASM のヒープ再利用も検査し、
`TSUZURI_ASAN=1 ASAN_OPTIONS=detect_stack_use_after_return=1 node tests/control.mjs target/release/tsuzuri`
で追加のメモリ検査ができます。
`benchmarks/run-control.mjs` は同じ ABI の C／C++／Rust と、
match／if／`Sub.sub` の末尾再帰を含む七つの matched workload を比較します。
小さい入力は独立した BigInt 参照結果とも照合し、共有環境では `--quick` で正しさだけを検査します。
`--baseline` で改善前のコンパイラも同じプロセス内にリンクして交互に測定できます。
`tests/tail_recursion.rs` と `tests/fixtures/control/Recursion.tz` は、SSA の配置に加えて
負のカウンター・8／64／128-bit の折り返し・可変引数のスナップショット・左右の副作用・trap・
IEEE の加算順序・所有値の解放・借用付き／非末尾呼び出しの通常経路を検査します。

`tests/features.mjs` は `tests/fixtures/<suite>` の機能別 fixture（可視性など）を
native／WASM の `-O0`／`-O3` で実行し、ネイティブの確保追跡で各呼び出し後の解放を確認し、
WASM がインポートを持たないことを検査します。第 2 引数に suite 名を渡すとその suite だけを実行します。
`node tests/features.mjs target/release/tsuzuri option_result` は標準 builder の短絡・反復・部分関数のトラップ、
参照経由のパターン束縛・Copy の独立性と所有 payload の反復解放を検査します。

任意の実ブラウザー検証:

```sh
TSUZURI_BROWSER="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
  node tests/examples.mjs target/release/tsuzuri
```

専用の一時プロファイルで headless Chrome を起動し、読み込み成功と UI の有効化を確認します。
既存のブラウザー・プロファイルは使いません。

新機能では構文、型、LLVM、ホスト ABI、エラー、資料の関連面を同時に更新してください。
言語の値の意味を変える最適化は不可です。特に短絡評価、トラップ、符号付きゼロ、
overflow、評価順序を両ターゲットで確認します。
型付き IR に必要な不変条件は、コード生成時のキャストではなく型検査で保証します。

## 初版の次に必要な設計

共有可変キャプチャ、外部パッケージ、効果の型付けは未実装です。
例外は関数本体の中の字句的な脱出だけで、関数・ラムダの境界を越える伝播と巻き戻しは未対応です。
借用record・単一regionの名前付き契約・再帰的なヒープ型・利用者定義のDropは実装済みですが、独立した複数regionのfield別追跡、
トラップ時の巻き戻しと解放、一般的なホストをまたぐ所有権は未対応です。
これらを追加するときも、寿命・ホスト境界・失敗モデルを型検査と一緒に設計する必要があります。

OS API・ハッシュコンテナ・文字列補間は実装済みですが、Windows の OS API（G10。現在は `E2002`）、WASI preview2 とコンポーネントモデル（E13）、ネットワーク（E09）、ハッシュコンテナの SIMD による群探査（F08）、書式指定の grapheme cluster 幅（D09）は未実装です。
