# PB05: デバッグビルド用の高速バックエンド

本文の起票時の設計（Phase 1 だけ、D13 は行わない、など）と手順は調査履歴です。現行の方針・決定事項・範囲・停止条件・テストコマンドは「着手時の再検討（2026-10-17）」と末尾の「実装と検証」を優先します。

| 項目 | 内容 |
| --- | --- |
| ID | PB05 |
| 分類 | ビルド速度 |
| 優先度 | P2 |
| 規模 | XL |
| 依存 | PB01, PB02 |
| 関連 | PB03, PB04, PB07, G08, G04 |
| 状態 | done（全フェーズ実装・検証済み。D12 は行わず、既定は `llvm`。速度は共有機の参考値で、Windows のホストでは `--backend fast` が `E2000`） |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`）。2026-10-17 に HEAD `9dd0fbbc` で再検討（「着手時の再検討」） |
| 承認 | 2026-10-17 の全フェーズ実装依頼で D1（第二のバックエンドとして Cranelift を採る。チケット全体）、D2（入力を Tsuzuri が出す `-O0` の LLVM IR テキストにする）、D3（Cranelift 系 crate・`target-lexicon`・Phase 2 の `gimli`/`object` の追加と cargo feature `fast-backend`）、D9（DWARF の行情報）、D13（直接の WASM 生成の評価と、ゲートを通った場合の実装）を含む全フェーズを承認済み（測定で効果を確認できない既定の切り替えは行わない）。D12（既定のバックエンドの変更）は本チケットでは行わず、既定は `llvm` のまま |
| 手本にする既存実装 | LLVM 以外の出力と対応外の早期拒否: `src/gpu.rs` の `extract_kernel`・`GpuKernel::wgsl`・`wgsl_type`。外部ツールの起動と一時ディレクトリ: `src/driver.rs` の `build_complete`・`tool`・`TemporaryDirectory`。CLI の値付きオプション: `src/main.rs` の `--cpu` の分岐と `BuildOptions::cpu`。cache の鍵: `src/cache.rs` の `build_key`。trap の E2E: `tests/e2e.mjs` の `traps`（SIGILL・SIGTRAP を `_Exit(86)` にする C harness） |
| 主な影響ファイル | `Cargo.toml`, `Cargo.lock`, `src/lib.rs`, `src/main.rs`, `src/driver.rs`, `src/cache.rs`, `src/backend/mod.rs`・`ir.rs`・`layout.rs`・`lower.rs`（新規）, `tests/fast_backend.rs`（新規）, `tests/fast_backend.mjs`（新規）, `docs/architecture.md`, `docs/language.md`, `docs/benchmarks.md`, `README.md`, `_perfs/README.md` |
| 計測対象 | PX03 の合成プロジェクト（`benchmarks/build/generate.mjs`、PX03 で新規）の native `-O0` の variant `build`・`build-edit-body`・`build-edit-signature`、`examples/hello`・`examples/computations` の `-O0` ビルド、生成コードの実行時間（`benchmarks/run-control.mjs` の `-O0` 相当） |

## 目的

デバッグビルド（native `-O0`）のコード生成で Clang を使わず、Cranelift で object を作って、編集→ビルド→実行の待ち時間を縮める。
Go や Zig の自前のバックエンド、Rust の Cranelift によるデバッグビルドと同じ考え方である。

- 入力は型付き IR ではなく、Tsuzuri が今も出している `-O0` の LLVM IR テキスト（連結されたランタイムの `.ll` を含む）とする（D2）。
  所有・解放・trap・評価順序の lowering は `src/llvm*.rs` の 1 か所に残し、新しいバックエンドは LLVM の命令を一つずつ Cranelift IR に写すだけにする。
  意味の一致を構成で保ち、差分テストは翻訳の誤りだけを探せばよい。
- Phase 1 は明示の `--backend fast`（新規）だけで選ぶ。自動では選ばず、既定も変えない（D12）。対応しない組み合わせと命令は
  E2000・E2002 で拒否し、黙って違う意味のコードを作らない。
- 実装者は Phase 1 だけを実装する。Phase 2（DWARF の行情報・`--trap-info`・`test` コマンド）と直接の WASM 生成（D13）は人間が求めた場合だけ着手する。

## 着手時の再検討（2026-10-17）

2026-10-17 の依頼で、全フェーズ（Phase 1・Phase 2・D13 の評価と、ゲートを通った場合の実装）を実装する。起票時の「Phase 1 だけ」「人間が求めた場合だけ」と
`要承認` の注記は今回の依頼には適用せず、意味の保持・既定を変えない・測定で決める条件は維持する。以下の「現状と計測（HEAD `f8dc655`）」「設計」「実装手順」
「決定事項」は起票時の記録で、このセクションと食い違う箇所はこのセクションが優先する（旧 `_docs/` のパスは `_tsuzuri/language-reference/` に読み替える。GUIDE §8.1）。
着手時の HEAD は `a2313d19`（main `9dd0fbbc` + 第4フェーズの計測の足場）。偵察は、保存した release コンパイラ（`target/perf/phase4-baseline/tsuzuri-before`、SHA-256 `5e1b9687…`）、
Apple M1 Max（10 コア）、macOS 27.0.1、Apple clang 21.0.0、rustc 1.98.1（Homebrew。MSRV の確認は rustup の 1.85.0 で行ったつもりだったが、`rustc` が PATH の Homebrew のものになっていた。2026-10-19 の訂正は「レビューの指摘と修正」の N3）、Node 20.19.6、LLVM 21 の単体ツールで行った。
共有機で 1 分の load average は 4〜30 だったので、時間は参考値である。生データと script は `target/perf/PB05/`（git 管理外）。

### 起票時の記述と現行のコード

| 起票時 | 現行 | 影響 |
| --- | --- | --- |
| 起票の HEAD は `f8dc655`。PB01・PB02・PX03 を前提とする | 第2・3フェーズが実装済み（PB01〜PB04、PX01〜PX03、PM01・PM05〜PM08、PR01〜PR06、G10 など）。`src/llvm*.rs` は 21,797 行（`src/llvm.rs` 8,997）、`src/driver.rs` 7,063 行で `build_complete` は約 1,280 行 | 起票時の行番号・件数・「まだない」は使わない。D2（lowering を二重に持たない）の理由は強まった（起票時は 10,888 行） |
| `EmitOptions { entry, wasm, debug_output }` と `external_numeric: bool`（numeric だけを宣言にする） | `EmitOptions { entry, wasm, wasm_simd128, debug_output, runtime_objects, allocator, f16 }`。`runtime_objects` が **numeric と math** を宣言だけにする（`src/runtime_cache.rs` の `numeric_declarations`・`math_declarations`・`math_source`）。連結する runtime は `RuntimeSymbols`（記号の集合）で決まる | 名前を現行に合わせる。fast も `runtime_objects` の条件を LLVM 経路と同じにして、Clang に渡すはずの IR をそのまま翻訳する（意味の一致が構成で保たれる） |
| `driver::runtime_object(cache, unit, source, input, args, directory, messages)`。unit は `numeric`・`native-c` | private な `compile_runtime_object(unit, source, input, args, directory, identity, hint, messages)`（`src/driver.rs`）。unit は `numeric`・`math`・`native-c`（`task.c` に `os.c`・`net.c`・`async.c`・`gpu.c`・`trap.c`・`cpu.c`・`io.c`・`arguments.c` を連結）・`wasi`・`wasm-threads`・`stack-debug`。配布物は 52 個の object を持つ（`runtime_cache::prebuilt`）。`identity: None`（`--no-cache`）でも毎回 Clang で作る形で動く | fast も同じ関数を同じ引数（`-O0` …）で呼ぶ。object と cache は LLVM 経路と共有される |
| native の実行ファイルは `clang -x ir module.ll` で compile と link を 1 回で行う | 同じ。引数は `-x ir -Wno-override-module -O0 -fPIC module.ll -o artifact -lm -Wl,-dead_strip -Wl,-u,_tz_<export> -x none numeric.o math.o -x none task.o …`。runtime object を使う条件は `options.cache && !debug_info && !trap_info && !trap_return && !wasm_threads && !stack_checks` で、native の実行ファイルと wasm32 の `.wasm` だけ。`--emit object` は `module.o` と `task.o` を `clang -r -nostdlib [-Wl,-keep_private_externs]` で結合する | fast は `module.o` を in-process で作り、`-x ir … module.ll` を `module.o` に替えた同じ link コマンドを使う。`module.ll` は書かない |
| 再現: hello の IR は 3,126 bytes、命令は 14 種、intrinsic は `@llvm.trap` だけ | `examples/hello`（`IO.write_line` を使う）の `build -O0` の module IR は 144,918 bytes・3,559 行・124 define・約 2,500 命令で、`run` は 160,789 bytes（`closure`・`heap-native`・`integer`・`string`・`string_v128`・`utf8string` の runtime が入る）。102 の project の生成コードに 49 種、runtime の `.ll` に 48 種、合わせて 51 種の命令・47 種の intrinsic・23 種の vector 型が現れた | 手順 2 の棚卸しは「再現」の `grep` では足りない。D6 の外の構成要素を下の表で決める |
| 診断の表: `--backend fast` は `-g`・`--trap-info` を E2000 で拒否し、`run` は Phase 1 で対応。`--trap-info` は Phase 2（trap 位置の表を Cranelift から作る） | `tsuzuri run` は常に `trap_info: true`（`src/main.rs` の `trap_info: trap_info \|\| action == Action::Run`。起票時の HEAD にもあった）。trap-info の計装は IR の書き換えである（`src/llvm_traps.rs`: 呼び出しに `i32 %tz.context` を足す、`@tz.trap.report(i32 site)` が `switch` で `@tz.trap.message.N` を `write(2, …)` する、`.trap.json` の sidecar）。機械語の位置表は使わない | 起票時の表では `run --backend fast` が自己矛盾だった。`run` と `--trap-info` は Phase 1 から対応する（翻訳の入力が IR のままなので追加の機構は要らない）。Phase 2 の「`--trap-info`」は消える。ただし `trap_info` では `runtime_objects` が false になり、numeric.ll・math.ll が IR に入って翻訳される |
| D6: `volatile`・atomic は E2002（出なければ実装しない）、vector は 128 bit 以下だけ、`half` と `byval`・`sret`・`fastcc` は停止条件 3 | 実 IR に出る: atomic（`atomicrmw` 7 種・`cmpxchg`・`fence acquire`・`load atomic`・`store atomic`。Rc・`Atomic`・`Mutex`・`Channel`・`heap-small.ll`・`heap-counting.ll`）、`volatile`（`math.ll` の load 9・store 23）、`thread_local`（Async の executor、`heap-small.ll`）、vector（全 string module の `string_v128.ll`: PR05 が `<16 x i32>`・`shufflevector`・`llvm.vector.reduce.add` を使う。user の SIMD は最大 `<32 x i32>`）、`half`（aarch64 の既定は `F16Path::Hardware`）、`fastcc`・`byval`・`sret`（numeric.ll の内部関数） | D6 を拡張する（下の表）。停止条件 3 は止めずに個別の方針で解く |
| D8: 集約値は scalar の列に平らにする | SSA の集約値の leaf 数は 1〜16 が大半（(型, module) の組で 2,955）、17〜64 が 28、`[1024 x i64]` を値で返す呼び出しが 1（`fixed_arrays`。A16） | 二層にする: 小さい集約は列、大きい集約は stack 上の値（by-ref） |
| D7: 手書きの `.ll` と `math.ll` は IR と一緒に翻訳し、`numeric` は PB01 の object | math も PB01 Phase 3 で外部 object になった（`sqrt`・`fabs` の小さい wrapper だけ IR に残る）。`--no-cache`・`run`（`trap_info`）・`-g`・`--emit llvm\|object` では numeric.ll（13,843 行）と math.ll（5,653 行）が IR に入る。PM08 が整数の表示を `integer.ll` に分けたので、整数だけを表示する program では numeric は入らない | translator は numeric.ll・math.ll も翻訳できる必要がある（`fastcc`・`byval`・`sret`・`weak hidden`・`volatile`・TBAA などの metadata・`icmp samesign`）。C との境界の signature は scalar と ptr だけ |
| `BuildOptions` は 10 field、`optimization` は数値 | 22 field（`allocator`・`freestanding`・`strip`・`wasm_opt`・`triple`・`trap_return`・`wasm_jspi`・`wasm_webgpu`・`wasm_net`・`wasm_host`・`wasm_max_memory`・`wasm_stack_size` …）、`optimization` は `OptLevel`（`O0`〜`O3`・`Os`・`Oz`）。`BuildOptions { … }` の構築は 80 か所で、全 field を書くのは `src/main.rs` の `parse_arguments` の 1 か所だけ（残りは `..options`・`..BuildOptions::default()`） | `backend` を足す変更は `Default`・`main.rs` の 1 か所・`validate` で済む。E2000 の文言は `{flag}` で `-Os`・`-Oz` を含める。他チケットが `BuildOptions` に field を足すと `parse_arguments` が衝突する |
| `build_key` はバックエンドを区別する入力がまだない。`BuildOptions` の `Debug` 表示に入るので鍵は自動で分かれる | 確認した（`src/cache.rs` の `build_key` は `hash.field("options", format!("{options:?}"))`）。ほかに IR の全文・compiler binary の digest・Clang の同定・`KEY_ENVIRONMENT`・`TSUZURI_LINKER` が入る。runtime object の鍵（`runtime_key`）は source・引数・Clang の同定で、backend を含まない | `build_key` は変えない。runtime object の cache は両バックエンドで共有してよい。初回は全 entry が 1 度 miss する（`compiler-binary` の digest が変わる時と同じ） |
| Clang の処理は `-O0` で 3.70 s、`-O3` で 7.62 s（合成 1,000 module、124,003 行、IR 約 29 MB） | 現行の生成器（139,010 行）の `-O0` の edit→build（cache あり・runtime object は warm・load 約 30 の共有機）は total 8.5〜9.0 s: `tool.clang` 4.3〜4.6 s、`emit` 2.1〜2.3 s、`cache` 0.4〜0.5 s、`check` 0.57〜0.6 s、`load` 0.25〜0.38 s、`specialize` 0.24 s。module IR は 37.4 MB・56,186 define・953,775 命令（PB03 の剪定後）。200 module は total 1.6〜1.8 s、`tool.clang` 0.90〜0.97 s、IR 7.7 MB・11,965 define・197,881 命令 | 削れる上限は `tool.clang` の約 50〜55%（compile が約 85%、link が約 15%）。`emit` と `check` は減らない。G3（124K 行を 1 s 以下）は PB05 だけでは届かない |
| 小さなプログラムは Clang の起動とランタイムの compile が支配的で、その大半は PB01 の対象 | hello の `-O0` の edit→build（cache あり）は total 280〜460 ms で、`tool.clang` は 75〜97 ms、`cache` 段（system clang の同定）が 110〜320 ms。新しく link した binary の初回起動は macOS が 310〜370 ms かける（2 回目は 40〜110 ms） | 小さい project の fast の利得は 70 ms 程度までで、G1 は大きい project でだけ意味がある。`run`・`--no-cache` で numeric・math を IR で持つ program は、Clang の `-O0` の compile が numeric.ll で約 180〜330 ms、math.ll で約 220〜300 ms 増える。fast なら翻訳（見積り 数十 ms）になる |
| D11: wrapper は `tsuzuri` を包み、`build`・`run` の `-O0` に `--backend fast` を足す | native の E2E の多くは `--emit llvm` の IR を suite 自身が `process.env.TSUZURI_CLANG ?? "clang"` で compile する（`@malloc` を tracked に置換した text）。調べた 28 の suite（`features.mjs` の 58 suite を含む）で、`tsuzuri` の `build`・`run` のうち wrapper が掴める `-O0` は合計 約 190 件、clang への `.ll` の `-O0` の compile は約 410 件（`features.mjs` だけで 138） | wrapper を 2 経路にする（D11 の更新） |
| 「Phase 2（DWARF・`--trap-info`・`test`）と D13 は人間が求めた場合だけ」 | 全フェーズの実装を依頼された | Phase 2・3 も実装・判断する（下の WP） |
| `--emit exe\|object\|llvm\|header\|wasm\|wgsl`、`--target native\|wasm32`、`-O0`〜`-O3` | `--emit` は 14 種（`shared`・`bindings-js\|cs\|py\|cpp`・`spirv`・`spirv-relaxed`・`wgsl-relaxed` を含む）、`--target` は native・wasm32・wasm64・TRIPLE（G15）、`-Os`・`-Oz`、`--cpu native`・`--freestanding`・`--allocator system\|host\|counting\|small`・`--trap-mode return`・`--strip`・`--wasm-opt`・`--link`・`-l`・`-L` | E2000 の表を作り直す（下の表） |
| 段名 `backend.parse`・`backend.lower`・`backend.object` | `TSUZURI_TIME_PASSES` は JSON 1 行（`load`・`cache`・`parse`・`check`・`specialize`・`closures`・`ownership`・`emit`・`tool.<名>`）。worker thread は timings に触れず、並列の段は呼び出し側 thread の実時間で測る（`src/parallel.rs` の規約）。`tests/time_passes.rs` は「段の和 ≤ total」を検査する | 段は `backend.scan`（逐次）・`backend.compile`（並列の解析 + lower + Cranelift）・`backend.object` にする |
| 停止条件 1・2: `rust-version = "1.85"` で build できる Cranelift が無い、C/C++ を compile する `build.rs` がある | 0.121.2（`rust-version` 1.85.0）が 1.85 以下の最新（0.122.0 は 1.86.0 を要求）。cargo 1.85.0 の `cargo check` が通り（21 s。ただし rustc は Homebrew の 1.98.1 だった。本物の rustc 1.85.0 でも依存が build できることは、N3 で確かめ直した）、依存は 31 package（30 crate 名）で、C/C++ を compile する `build.rs` も network を使う `build.rs` もない | 停止条件 1・2 は解消 |
| 停止条件 8: Cranelift の object を Clang で link して linker の warning が設定で消せないなら止める | Mach-O の object には `LC_BUILD_VERSION` がなく、`ld: warning: no platform load command found in '…', assuming: macOS` が出る。`object::write::Object::set_macho_build_version` を呼べば消える（スパイクで確認）。`.note.GNU-stack` は `cranelift-object` が出す | 停止条件 8 は解消した形で組み込み、`tests/fast_backend.mjs` が stderr が空であることを検査する |

### 実 IR の棚卸し（D6 に無い構成要素と処理方針）

方法（手順 2 の前倒し。script は `target/perf/PB05/inventory.mjs`・`digest.mjs`・`aggregates.mjs` で、WP1 で `tests/fast_backend.mjs inventory` にする）: ①`examples/*`・`tests/fixtures/*` の 102 project の
`--emit llvm -O0` の IR から runtime の `.ll` の関数・global を除いた生成コード（define 44,777・call 146,397・命令 約 100 万）、②`src/runtime/*.ll` 30 ファイルを 1 つずつ、
③native の `-O0` の実行ファイルと `run` の module IR（Clang 呼び出しの前に複写。hello・computations・tasks・control・io・polymorphism・point・functional・currying）。
標本なので網羅は保証しない。translator が知らない命令・intrinsic・属性は E2002 にし、parity の suite で見つかったものを足す。結果（命令 51 種・intrinsic 47 種・vector 型 23 種の和集合）のうち D6 に無いもの:

| 構成要素 | 実測の出どころ | 方針 |
| --- | --- | --- |
| 集約値の SSA（`phi`・`select`・call の引数と戻り・`insertvalue`・`extractvalue`） | 全 module。最大 1,024 leaf | D8 を二層にする（下） |
| `i128` の演算（加減乗・shift・比較・`phi`・`sext`・`zext`・`trunc`・`sdiv`・`udiv`・`srem`・`urem`） | 19 project、`integer.ll`・`numeric.ll` | Cranelift の `i128`。割り算・剰余は Cranelift が `__divti3`・`__udivti3`・`__modti3`・`__umodti3` の libcall にする（LLVM と同じ名前。MSVC は `windows_abi` が IR に実装を足す） |
| `llvm.{s,u}{add,sub,mul}.with.overflow.{i8,i32,i64,i128}`・`llvm.{s,u}{add,sub}.sat.*` | 整数の checked・saturating の builtin | Cranelift の `*_overflow`・`*_sat`。そのスカラー幅で未対応なら手書きの列（`i128` の乗算も手書き） |
| `llvm.fshl/fshr`・`ctpop`・`ctlz`・`cttz`・`bswap`・`bitreverse`・`abs`・`smin/smax/umin/umax`・`scmp` | 整数の builtin、numeric.ll、math.ll | `rotl/rotr`・`popcnt`・`clz`・`ctz`・`bswap`・`bitrev`・`iabs`・比較と `select` |
| `llvm.floor/ceil/trunc/roundeven/sqrt/fabs/copysign/fma/is.fpclass`・`frem` | `Math`、math.ll | `floor/ceil/trunc/nearest/sqrt/fabs/fcopysign/fma`。`is.fpclass` は整数の bit 検査。`frem` は `fmod`・`fmodf` の呼び出し（LLVM と同じ）。FMA がない CPU の `fma` は libm の `fma`（LLVM の `-O0` と同じ） |
| atomic: `atomicrmw`（add・sub・and・or・xor・xchg・umax）・`cmpxchg`・`fence`・`load atomic`・`store atomic` | Rc・`Atomic`・`Mutex`・`Channel`・`heap-small.ll`・`heap-counting.ll` | Cranelift の `atomic_rmw`・`atomic_cas`・`atomic_load`・`atomic_store`・`fence`。Cranelift の順序は seq_cst だけで、要求より強い（意味は保たれる）。`cmpxchg` は strong で実装する |
| `volatile` の load・store | math.ll、`heap-wasm-threads.ll` | 通常の load・store（最適化しないので消えない） |
| `thread_local` の global と `llvm.threadlocal.address` | Async の executor（`tz.async.*`）、`heap-small.ll` | `declare_data(tls = true)` と `tls_value`。aarch64 の Mach-O で動作を確認した（下）。ELF・COFF は `cranelift-object` が対応しているので実行は CI で確かめる |
| `weak` linkage と `weak hidden` | `tsuzuri_alloc`・`tsuzuri_free`（host ABI）、numeric.ll の入口 | `Linkage::Preemptible`（weak）。`weak hidden` は記号の scope を `object::write` で hidden にする。`linkonce*`・`common`・`extern_weak`・`appending`・`alias`・`ifunc`・`section`・`comdat` は実 IR に無く E2002 |
| `fastcc`・`tail`・`byval`・`sret` | numeric.ll・`heap-small.ll` の内部関数 | module 内部の関数だけ（C との境界には出ない）。`fastcc`・`tail` は無視、`sret` は ptr 引数、`byval(T)` は呼び出し側で stack に複写した領域の ptr を渡す。宣言のみの関数と外から呼ばれる定義の `byval`・`sret`・`inreg`・`musttail`・`invoke`・`va_arg` は E2002 |
| vector（`<2 x i1>`〜`<32 x i32>`）・`shufflevector`・`extractelement`・`insertelement`・`bitcast <N x i1> to iN`・`llvm.vector.reduce.add` | `string_v128.ll`（PR05）の全 string module、user の SIMD（F03・F04）、`@cpu` の version | lane の scalar 化（SIMD 命令は使わない。Cranelift は 128 bit までで、x86-64 の既定の ISA 拡張に依らず決定的）。値は lane の列、memory は自然な layout。`target-features` は無視する（全 version が同じ意味） |
| `half`（`fadd`・`fcmp`・`fpext`・`fptrunc`） | aarch64 の既定の `F16Path::Hardware` | fast の駆動は `F16Path::Soft`（`tz_soft_*` の整数の実装。D10 は全経路でビットが同じと定める）を渡す。translator は `half` を E2002 にする |
| `llvm.trap` と `unreachable` | 13,442 組 | aarch64: `debugtrap` の後に `trap`、x86_64: `trap`。スパイクで、LLVM の `llvm.trap` と `debugtrap`+`trap` が SIGTRAP、`trap` だけは SIGILL と確認した。直後でない `unreachable` は `trap` |
| metadata（`!tbaa`・`!alias.scope`・`!noalias`・`!llvm.loop`・`!prof`・`!nonnull`・`!range`・`!tbaa.struct`）と `llvm.assume`・`llvm.lifetime.*`・`llvm.experimental.noalias.scope.decl` | numeric・math・生成コード | 読み飛ばす（D6 のまま） |
| 命令の flag（`nsw`・`nuw`・`exact`・`inbounds`・`samesign`・`nneg`・`disjoint`）と属性（`noundef`・`nonnull`・`noalias`・`nocapture`・`readonly`・`align`・`dereferenceable`・`alwaysinline`・`noinline`・`cold`・`memory(…)`・`"target-features"`・`"tz-cpu"`） | 全 module | 捨てる。fast-math の flag は実 IR に出ていないので E2002 のまま。`signext`・`zeroext` は C との境界で `AbiParam::sext`・`uext` に写す |
| `switch`（`i1`・`i8`・`i16`・`i32`・`i64`）、定数式 `getelementptr (T, ptr null, i32 1)`（sizeof の idiom が 18,330 回）・`ptrtoint`・`inttoptr` | match・case・allocation の大きさ | `cranelift_frontend::Switch`（`i1` は `brif`）。定数式は layout で畳む |
| global の初期化子の記号参照（関数・data のアドレス） | closure descriptor・vtable（PM02 の静的 descriptor でも同じ） | `DataDescription` の `write_function_addr`・`write_data_addr`（再配置） |
| 宣言だけで呼ばれない intrinsic（`llvm.vector.reduce.add.v8i16` が全 module に 1 行ある） | 全 module | 呼ばれるまで型を解釈しない。到達性で、root（外から見える define・アドレスが取られる関数・初期化子から参照される関数）から届く関数と global だけ翻訳する |

alloca はすべて `align 16`（53,016）か `align 8`（23）で、Cranelift の slot の alignment（16）を超えるものはない。C との境界（`declare` と外から見える `define`）の型は `ptr`・`i32`・`i64`・`float`・`double`・`void` だけで、
集約・`i128`・vector は 1 つもない（D8 の前提は成り立つ）。`llvm.frameaddress` は wasm だけ、`llvm.dbg.*` は `-g` だけに出る。

### 偵察で測った事実（2026-10-17）

**時間のモデル**（native `-O0`、cache あり・runtime object は warm・source を実際に編集して再 build、`TSUZURI_TIME_PASSES=1`、共有機。参考値）:

| project | module IR | total | `tool.clang` | `emit` | `cache` | 残り（`load`・`check`・`specialize`…） |
| --- | --- | --- | --- | --- | --- | --- |
| `examples/hello` | 145 KB・124 define・約 2,500 命令 | 280〜460 ms | 75〜97 ms | 2 ms | 110〜320 ms | 約 30 ms |
| 合成 200 module | 7.7 MB・11,965 define・197,881 命令 | 1.6〜1.8 s | 0.90〜0.97 s | 0.21 s | 0.19 s | 約 0.2〜0.3 s |
| 合成 1,000 module | 37.4 MB・56,186 define・953,775 命令 | 8.5〜9.0 s（load 約 30） | 4.3〜4.6 s | 2.1〜2.3 s | 0.4〜0.5 s | 約 1.2 s |

- 合成 200 module の `tool.clang` を compile と link に分けて replay すると、compile が 0.93〜1.05 s、link が 0.15〜0.24 s。`-ftime-report` では pass の合計が 1.47 s で（AArch64 の `-O0` は GlobalISel）、
  IRTranslator 10%・LoopSimplify 7%・InstructionSelect 7%・Prologue/Epilogue 6.5%・Legalizer 4.6%・Fast RA 4.3% など。LLVM の命令 1 個あたり約 4.8 µs。link は PB05 の後も Clang のまま残る。
- wasm32 `-O0`（D13 の証拠）: hello は total 260〜280 ms のうち `tool.clang` 43〜55 ms・`tool.wasm-ld` 33 ms。合成 200 module は total 1.83〜2.2 s のうち `tool.clang` 1.05〜1.10 s（57〜60%）・`tool.wasm-ld` 34〜55 ms（2〜3%）。
- 小さい program の edit→build→run は、build の外の固定費が大きい: `cache` 段（system clang の同定）が 110〜320 ms、新しく link した binary の初回起動が 310〜370 ms（同じ binary の 2 回目は 40〜110 ms）。
  PB05 が削れるのは Clang の 75〜97 ms のうち翻訳に替わる分だけで、小さい project の G1 の利得は小さい。合成 200・1,000 module では total の 30〜40% の短縮が見込める（見積り。`emit`・`check`・link は残る。WP9 で実測する）。
- `run`・`--no-cache`・`-g` で numeric.ll・math.ll が IR に入る program の Clang `-O0` は、numeric.ll の compile だけで 180〜330 ms、math.ll で 220〜300 ms。
  `run -O0 --no-cache` の hello は total 0.82〜1.0 s（`tool.clang` 0.40〜0.43 s。C ランタイムの compile と module の 2 回の Clang）。

**Cranelift の入手性とスパイク**（`target/perf/PB05/proto/`、git 管理外。cargo 1.85.0 で build。rustc は Homebrew の 1.98.1 だった。N3 参照）:

- `rust-version` が 1.85 以下の最新は `cranelift-{codegen,frontend,module,object}` 0.121.2（`cargo info` で 0.121.x が 1.85.0、0.122・0.123 が 1.86.0、0.124 が 1.87.0、…、0.136 が 1.96.0）。
  MSRV 対応の resolver は `target-lexicon` 0.13.5・`gimli` 0.31.1・`object` 0.36.7・`regalloc2` 0.12.2 を選び、`cargo tree -e normal` は 31 package（30 crate 名。`hashbrown` が 2 版）で、いまの `tsuzuri` の 9 行に足される。
  `cc`・`cmake`・`bindgen` は無く、`build.rs` を持つのは cranelift-codegen・cranelift-assembler-x64・cranelift-isle（Rust の生成器）、`object`・`anyhow`・`libm`・`crc32fast`・`target-lexicon`（rustc の version の検査）だけで、
  C/C++ を compile する `build.rs` も network を使う `build.rs` もない。ライセンスは Apache-2.0（LLVM-exception つき）・MIT・Zlib・Unlicense などで、feature を既定にして配布に含める時（D12 以降）は表記が要る。
- 4 crate + 依存の release build（既定の profile で LTO なし、`-j4`、load 約 30）の費用。スパイク（自分のコードは数百行）の binary は strip 後、`arm64` だけが 2.5 MB（wall 43 s・CPU 76 s）、`x86` + `arm64` が 4.3 MB（57 s・102 s）、
  4 arch 全部入り（x86・arm64・s390x・riscv64）が 5.6 MB（50 s・114 s）で、`tsuzuri` は 9.5 MB。`thin` LTO・`codegen-units = 1` の本物の値は WP2 で測る。
- 速さ: 12,000 関数（合成 200 module の define 数）× 約 27 CLIF 命令（合計 約 33 万、合成 200 module の LLVM 命令の約 1.7 倍）を `opt_level = none` で build + compile するのに、1 thread で 33 + 386 ms、4 thread で wall 147 ms、
  8 thread で wall 97 ms（load 約 20）。`define_function_bytes` の定義が 1 ms、object の書き出しが 3 ms。1 命令あたり約 1.2 µs で、Clang（LLVM の命令 1 個あたり約 4.8 µs）の約 1/4。ただし計測したのは Cranelift の back end だけで、
  IR の解析と翻訳（集約の展開で CLIF の命令は LLVM の命令より増える）の時間は含まない。合成 1,000 module（約 95 万命令）の外挿は back end が 1 thread で約 1.5 s、8 thread で約 0.4 s。IR の解析（37 MB）は別に 0.2〜0.4 s と見積もり、WP9 のゲートで実測する。
- Mach-O: Cranelift の object を `clang obj -o exe` で link・実行できる。`LC_BUILD_VERSION` が無いと `ld: warning: no platform load command found … assuming: macOS` が出るが、`set_macho_build_version` で消える。
  `MH_SUBSECTIONS_VIA_SYMBOLS` は付く（`-dead_strip` が関数単位で効く）。thread-local: `tls_model = macho` の `tls_value` で、main thread と worker thread の counter が別になることを実行で確認した。
  別の triple の object（x86_64・aarch64 の Linux の ELF、x86_64 の macOS、x86_64・aarch64 の Windows の COFF）も出して `llvm-objdump -h` で読めた（実行はできない。aarch64 の Windows は `CallConv::WindowsFastcall` と出るので未検証とする）。
- trap の signal（aarch64 macOS で実行）: LLVM の `llvm.trap` は SIGTRAP、Cranelift の `trap`（`udf #0xc11f`）は SIGILL、`debugtrap`（`brk #0xf000`）は SIGTRAP、`debugtrap` + `trap` は SIGTRAP。
- 並列: `Context::compile` の後に `ModuleReloc::from_mach_reloc` で再配置を worker で作り、`define_function_bytes` を関数の順に呼ぶ形が成り立つ（object は決定的）。`src/parallel.rs` の `ordered_for_each` が使える。

**既存 suite と parity（D11 の更新の根拠）**: 28 の suite を、baseline の compiler と clang の spy の下で実行し（全部成功。`utf_simd` は spy の置き場に `llvm-objdump` が無く落ちた。wrapper の置き場に LLVM の各ツールの symlink が要る）、
`tsuzuri build`・`run` の `-O0` で経路 A の wrapper が掴める件数と、clang への `.ll` の `-O0` で経路 B の wrapper が掴める件数を数えた（A / B）:
control 0/3、primitives 7/3、computations 5/10、strings 10/18、numeric_casts 3/3、integer_intrinsics 10/14、examples 9/0、io 14/21、tasks 4/12、display_parse 32/43、host_imports 1/3、os 24/43、e2e 23/15、json 0/2、
async 1/8、ffi_extensions 11/8、user_drop 1/4、trap_return 1/3、trap_return_sync 0/4、f16_hardware 2/10、cpu_dispatch 2/5、cpu_kernels 2/5、allocator 6/10、small_allocator 1/3、stack_overflow 9/10、integer_format 0/6、utf_simd 0/2、
features 9/138（58 suite・15,385 case）。合計は A が約 190 件、B が約 410 件。

- 経路 B は `TSUZURI_CLANG` に wrapper（sh script）を指す。`.ll` の入力が `-O0`（または `-O` なし）の呼び出しだけ、`.ll` を翻訳して `.o` に替え、残りの引数は本物の clang に渡す。suite の source は変えない。
  suite が `--emit llvm` で作る IR は runtime 全体を含む（numeric・math・`string_v128` を含む）ので、translator が numeric.ll・math.ll・vector を扱えることが前提になる。
  `tsuzuri` 自身の `-x ir` の compile（既定の LLVM 経路）も同じ wrapper で翻訳される。経路 A は `--backend fast` の driver の経路そのもの（link・runtime object・cache・E2005）を検証する。
- 翻訳だけを呼ぶ入口は、`tsuzuri` の隠しサブコマンド `__fast-object IN.ll -o OUT.o`（`--help` に出さず、`docs/architecture.md` に書く）にする。別の `[[bin]]` を足さず、検証する binary と同じものを使うため。
- 除外する suite: 機械語の pattern を検査するもの（`simd`・`wasm_simd`・`cpu_dispatch` の inspect・`cpu_kernels`・`utf_simd`・`f16_hardware`）、`-g`（`debug_info`・`debugger`）、wasm と host binding（`wasm_*`・`wasm64`・`net_wasm`・`bindings*`・`os-wasi-host`・`host_bindings`）、
  Clang の呼び出し・cache・同梱物そのものを検査するもの（`cache`・`prebuilt_runtime`・`bundle_identity`・`llvm_handoff`・`handoff_tools`・`targets`・`size`・`benchmark_*`）、native の build が無いもの（`lsp_sessions`・`docgen`・`repl`・`script`・`packages`・`frontend_cache`・`parallel_frontend`）、
  Phase 1 で E2000 にする組み合わせ（`trap_return*`・`--allocator small|counting`）。`stack_overflow`・`allocator`・`small_allocator` は経路 B の対象だが frame の大きさに敏感なので「監視」として結果を別に記録する。
- Node: CI の `targets.yml` は全 job が Node 24。今回の 28 suite は Node 20.19.6 でも成功したが、BigInt を多用する `math.mjs`・`display_parse_exhaustive.mjs` は Node 24
  （`npx --yes --package=node@24 node tests/<suite>.mjs …`）で実行する。native の parity に Node 24 が必須の suite は無い（`wasm64.mjs`・`wasm_heap.mjs wasm64` は wasm なので対象外）。

**option の組み合わせ**（Phase 1 は `--backend fast` に対して。`E2000` の文言は「診断」の表の形式）:

| 指定 | 現行 | Phase 1 | Phase 2 以降 |
| --- | --- | --- | --- |
| `-O` | build・run の既定は `-O3`、`test` は `-O0` | 省略は `-O0`。`-O0` だけ許し、`-O1`〜`-O3`・`-Os`・`-Oz` は E2000 | 同じ |
| `run` | 常に `trap_info`。一時 dir に build して実行し、trap は E2005（位置つき） | 対応。E2005 のメッセージ（signal 名と位置）を LLVM 経路と一致させる | |
| `--trap-info` | IR の計装と `<out>.trap.json` | 対応（IR のまま翻訳。numeric・math も IR で翻訳） | |
| `-g`・`--debug-info` | IR の `!DI*`・macOS は `dsymutil --flat`・MSVC は CodeView と PDB | E2000 | WP12: 行情報（gimli）。macOS は dsymutil。MSVC は E2000 のまま |
| `--cpu native` | Clang に `-mcpu=native`・`-march=native`、f16 の経路、cache の鍵に CPU の機能 | E2000（`generic` だけ） | 同じ（debug build に不要） |
| `--allocator` | `system`・`host`・`counting`・`small` | `system`・`host` だけ。`counting`・`small` は E2000 | WP15: `allocator.mjs`・`small_allocator.mjs` が翻訳で通れば許す |
| `--freestanding`・`--trap-mode return` | libc 不要の object、`trap.c` の境界 | E2000 | WP15 で評価（`--freestanding` は `memcpy`・`memset` の呼び出しが残るので、通らなければ E2000 のまま） |
| `--target` | native・wasm32・wasm64・TRIPLE | host の native だけ。wasm と TRIPLE は E2000 | wasm32 は D13（Phase 3） |
| `--emit` | 14 種 | `exe`・`object` だけ。ほかは E2000 | `shared` を WP15 で評価 |
| `--strip`・`--link`・`-l`・`-L`・`--debug-output` | link 引数・IR の `tz.debug.write` | 対応（link は Clang のまま） | |
| `--no-cache` | whole-build cache と runtime object cache を読み書きしない | 対応。`runtime_objects` は LLVM 経路と同じ条件（false）で、numeric・math は IR で翻訳。Clang を使うのは link だけ | |
| `test`・`bench` | `src/test_runner.rs` が別に `clang -x ir` と `test-runner.c`。`test` の既定は `-O0` | `--backend` は build・run だけ（`test`・`bench` は E2000） | WP13: `test` に `--backend fast` |
| `--wasm-*`・`--wasm-feature` | | E2000 | |
| Windows、または aarch64・x86_64 以外の host | | E2000（`--backend fast is not available on …`） | WP14: x86_64 の Windows |
| feature なしの build | | E2000（`rebuild with cargo build --release --features fast-backend`） | |

**D13（直接の WASM 生成）の評価**:

- 証拠: wasm32 `-O0` の edit→build で Clang は合成 200 module の total の 57〜60%（上）で、native の 50〜55% と同じ規模。hello では 約 18%（43〜55 ms／260〜280 ms）で小さい。`wasm-ld` は 2〜3% で残る。
  wasm 用の runtime IR（`wasm.ll`・`heap-wasm.ll`・`heap-wasm64.ll`・`heap-wasm-threads.ll`・`task-wasm.ll`・`sync-wasm.ll`・`channel-wasm.ll`）は vector・`fastcc`・threads 以外の atomic を使わず、wasm 固有の intrinsic は
  `llvm.wasm.memory.grow/size` と `cttz` だけ。string は `string_scalar.ll`（simd128 なし）で、native の前半（解析・layout・到達性・集約の分類・intrinsic の表）はそのまま共有できる。
  一方で `object` crate の `write` に wasm は無く（`coff`・`elf`・`macho`・`pe`・`xcoff` だけ）、Cranelift は wasm を出力しない。`wasm-encoder` など新しい crate は、今回追加を許された範囲（Cranelift 系・`target-lexicon`・`gimli`・`object`）の外なので、wasm の binary 符号化と再配置可能な object
  （`linking`・`reloc.CODE`・`reloc.DATA`）を自前で書く（形式は公開仕様で、`wasm-ld` が読む）。wasm には `run` が無いので利得は build→reload の待ちである。
- 判断: 技術的には成立し、大きい project では native と同程度（total の約 50%）の利得が見込める。ただし Cranelift を使えず、構造化された制御構造への変換・wasm の object writer・再配置が丸ごと新規で、
  利得は大きい project に偏る。native のゲート（WP9）を通らなければ前半の共有部分も意味がないので、**WP9 を通ってから着手し、WP16 で再確認する**（通らなければ D13 も取り下げて記録する）。
- 設計（WP17〜WP20 の素案）: 入口は `--backend fast --target wasm32 [--emit wasm|object]`（`-O0` だけ。`--wasm-feature`・`--wasm-host`・wasm64・threads・jspi・webgpu・net は E2000 のまま）。
  前半は共有する（`ir.rs`、wasm32 の DataLayout（ptr 4 byte・`i64:64`・`i128:128`）、到達性、集約の二層、intrinsic の表）。制御構造は、-O0 では任意の CFG を `loop { block … br_table }` の dispatch
  （基本 block ごとに case、`$pc` の local、phi は辺で local へ代入）に写す（reducible な if/else の畳み込みは計測してから）。SSA 値は local（stackify しない）、alloca は shadow stack
  （`__stack_pointer` を関数の入口で下げ出口で戻す）、集約は二層、`i128` は 2 つの `i64`（乗除は `wasm.ll` の `__multi3`・`__udivti3` …）、`llvm.*` は wasm 命令（`i64.clz`・`f64.sqrt`・`trunc_sat`・
  `memory.copy`・`memory.fill`・`memory.size`・`memory.grow`）。object は type・import（未定義の関数・`__stack_pointer`・`__linear_memory`・`__indirect_function_table`）・function・code
  （再配置の欄は 5 byte に pad した LEB128）・data・`linking`（symbol table・segment info・weak・hidden・undefined）・`reloc.CODE`・`reloc.DATA`
  （`R_WASM_FUNCTION_INDEX_LEB`・`TABLE_INDEX_SLEB`（関数 pointer は table index）・`MEMORY_ADDR_LEB`・`MEMORY_ADDR_SLEB`・`MEMORY_ADDR_I32`・`TYPE_INDEX_LEB`・`GLOBAL_INDEX_LEB`）。
  link は今と同じ `wasm-ld`（`--no-entry`・`--stack-first`・`--export=…`・`--max-memory`）、`numeric`・`math`・`wasi` は PB01 の object。検証は `WebAssembly.validate`、`WebAssembly.Module.imports(module)` が空（既定の import なし）、
  経路 B と同じ考えの wrapper（wasm32 の `-x ir` の呼び出しを翻訳）で `features.mjs` の wasm32 `-O0` と `wasm_*`・`wasm_memory.mjs` を実行する。規模は新規 約 3,500〜4,500 行（約 3 週間相当）。
  リスクは `wasm-ld` の再配置の細部、V8 の validate、dispatch 形式の実行速度（debug build なので許容し、計測だけ）。

### 決定事項の更新

- D1（Cranelift）: 維持。スパイクで、Cranelift 0.121.2 が `opt_level = none` で Clang の約 1/4 の命令あたりの時間で、関数単位に並列化でき、Mach-O・ELF・COFF の object を出せることを確かめた（back end だけの測定。ゲートは WP9）。
- D2（入力は `-O0` の LLVM IR テキスト）: 維持。補足: IR は文字列のまま in-memory で渡し（`module.ll` を書かない）、解析は逐次の走査（type・global・declare・define の header と本体の範囲）と、worker での本体の解析に分ける（D16）。
  `src/llvm*.rs` は変えない。`EmitOptions` に knob を足さず、f16 は既存の `F16Path::Soft`、vector は translator が lane に分けて扱う（PM03・PM04 が runtime の `.ll` を変えても追従できる）。
- D3（crate と feature）: 変更。`cranelift-codegen`・`cranelift-frontend`・`cranelift-module`・`cranelift-object` を `=0.121.2` で固定し（codegen の features は `std`・`x86`・`arm64` で、既定の `timing`・`host-arch` は切る。`unwind` は
  cranelift-object・module・frontend が必ず有効にする）、`target-lexicon` は 0.13（cranelift が要求する版）。`cranelift-native` は使わない（host の CPU の検出で出力が機械に依存する）。Phase 2 の `gimli` は、`unwind` が
  `gimli` 0.31.1（`read`・`write`・`std`）を既に引いているので、同じ版を直接の依存に宣言するだけで増えない。`object` も `cranelift_object::object` の再 export を使い、直接の依存は足さない。
  cargo feature `fast-backend`（既定は空）。MSRV の検証に `~/.cargo/bin/cargo +1.85.0 build --release --locked --features fast-backend` を手順に入れる（Homebrew の `cargo` は `+toolchain` を取らない。ただし、この機械では `cargo +1.85.0` が呼ぶ `rustc` は PATH の Homebrew のものなので、`RUSTC` も指定する。N3）。`x86`・`arm64` を両方入れるのは、
  別 arch の object を作る test（layout・ELF・COFF の smoke）を host に依らず実行するため。大きさの増加は WP2 で測り、大きすぎれば host の arch だけにする。
- D4（CLI）: 変更。`--backend llvm|fast`、既定は `llvm`。`build`・`run` が受ける（WP13 で `test`）。E2000 の表は上の「option の組み合わせ」。`-O` 省略は `-O0`。
- D5（Phase 1 の範囲）: 変更。範囲は上の表（`run` と `--trap-info` を含む。`-g`・`--cpu native`・`counting`・`small`・`--freestanding`・`--trap-mode return`・`--emit shared` は E2000）。
- D6（翻訳規則）: 拡張。上の「実 IR の棚卸し」の表を D6 の表に足す。表にない構文は E2002（`the fast backend cannot translate {construct} in @{function}; …`）。停止条件 3 は、棚卸しの結果で個別の方針を決めたので止めない。
- D7（ランタイム）: 変更。fast は LLVM 経路に渡る IR と同じ text を翻訳する（`runtime_objects` の条件は LLVM 経路と同じ）。numeric・math が IR に入る経路（`run`・`--no-cache`・`--emit object`・`--trap-info`）は翻訳し、
  cache ありの native の実行ファイルは PB01 の object を使う。`native-c` は常に PB01 の object。
- D8（集約値と ABI）: 変更。二層にする: leaf が `AGGREGATE_FLATTEN_LEAVES`（初期値 16。WP6 で frame の大きさと速さを見て決める）以下の SSA 値は scalar の列、超える値は stack slot 上の値で、
  `load` は複写、`insertvalue` は複写してから更新、`phi`・`select`・call はその領域の ptr を渡す。module 内の定義は同じ規則で引数・戻りを決め、C との境界は scalar と ptr だけ（実 IR の 1,651 の signature で確認済み）。
- D9（デバッグ情報）: 変更。Phase 2（WP12）で実装する。IR の `!DILocation`・`!DISubprogram`・`!DIFile` を読み、Cranelift の `SourceLoc` と `gimli::write` で `.debug_line` と `.debug_info`（CU と subprogram）を書く。
  変数・型の DIE は書かない（G16 の `$tag`・LLDB・natvis の契約は LLVM 経路のままで、`-g` の fast の制限として文書化する）。Phase 1 は `-g` を E2000 にする（起票時のまま）。
- D10（cache）: 維持。`build_key` は変えない。runtime object の cache は両バックエンドで共有する。
- D11（意味の一致の検証）: 変更。2 経路の wrapper（`tests/fast_backend.mjs wrapper <dir>` が作る）。経路 A は `tsuzuri` の wrapper で、`build`・`run` の `-O0`（または `-O` なし）に `--backend fast` を足して `exec`、
  経路 B は `TSUZURI_CLANG` の wrapper で、`.ll` を入力に持つ `-O0` の呼び出しを `tsuzuri __fast-object IN.ll -o OUT.o` で翻訳して `.o` に替え、ほかの引数を本物の clang に渡す（`-g`・`--target=`・`-fsanitize`・
  `-mcpu`・`-march`・`-flto` を含む呼び出しは触らない）。wrapper の置き場に LLVM の各ツールの symlink を置く（suite が `dirname(clang)` から `llvm-objdump` などを探す）。E2002 は経路 B では本物の clang へ fall back して件数と理由を log に書く
  （coverage の欠落として `docs/benchmarks.md` の PB05 節に報告し、suite の期待値は変えない）。経路 A は fall back しない（E2002 のまま失敗）。各 suite が翻訳した件数を表示し、0 件の suite は一覧から外す。
- D12（既定のバックエンド）: 変更なし。`llvm` が既定で、自動選択もしない。
- D13（直接の WASM 生成）: 評価の結果、ゲート（WP9）を通ってから WP17〜WP20 で実装する（上の評価）。
- D14（新設。並列と決定性）: 関数単位の解析・lower・Cranelift の compile を `src/parallel.rs` の `ordered_for_each`（worker の stack は 8 MiB）で並列にし、`define_function_bytes` を関数の定義順に呼ぶ。
  記号・data の順は IR に現れた順で、`HashMap` の反復順を使わない。同じ入力の object の SHA-256 は `TSUZURI_THREADS=1` と既定で一致する。段は `backend.scan`・`backend.compile`・`backend.object`。
- D15（新設。Cranelift の設定と object の後処理）: ISA は `isa::lookup(triple)` の固定の flag で、host の CPU を検出しない（x86-64 は baseline）。`opt_level = none`、`is_pic = true`、`enable_verifier` は `debug_assertions` の build だけ、
  `enable_nan_canonicalization = false`、`enable_probestack = true` と `probestack_strategy = inline`（`__cranelift_probestack` の外部記号を作らない）、`unwind_info = false`、`tls_model` は target の OS で決める。
  `regalloc_algorithm` は `backtracking` と `single_pass` を WP9 で比べて決める。後処理: Mach-O の `LC_BUILD_VERSION`（platform macOS、minos は arch の最小）、weak・hidden の記号の scope、`.note.GNU-stack`（出力済み）。
  Phase 1 は host の triple だけ。
- D16（新設。翻訳の単位）: 外から見える define・アドレスが取られる関数・global の初期化子から参照される関数を root にして、届く関数と global だけを翻訳する（`-O0` の LLVM が出す未使用の `internal` 関数は出さない。
  観測できる差は無く、object が小さくなる）。外から見える記号の集合は LLVM 経路と同じになる（生成コードの確認の条件）。

### 作業の順序（WP）

各 WP は小さな commit（`WP<n>: …`）で、それぞれの確認が通ってから次へ進む。`cargo test --locked <filter>` の `running N tests` の N を毎回見る。feature 付きの test は `--features fast-backend` を付けないと 0 件になる。

| WP | 内容 | 確認 |
| --- | --- | --- |
| WP0 | この再検討（docs だけ） | `git diff --check` |
| WP1 | 基準と棚卸し: `tests/fast_backend.mjs` の `inventory` mode（命令・intrinsic・型・属性・linkage の表を `target/perf/PB05/inventory.txt` に。compiler が出しうる `llvm.*` の静的な列挙つき）。既定の出力が変わらないことの比較 script（`examples/*`・`tests/fixtures/*` の `--emit llvm\|object`・実行ファイル・wasm を before と byte 比較。Mach-O の `LC_UUID`・署名は除く） | `node tests/fast_backend.mjs target/release/tsuzuri inventory` が成功し、表のすべての行に方針がある |
| WP2 | crate と feature: `Cargo.toml`・`Cargo.lock`・`src/lib.rs`・`src/backend/mod.rs`（`compile` は E2002 の仮実装）。M5 | `cargo build --release --locked`（feature なし・あり）、`cargo +1.85.0 build --release --locked --features fast-backend`、`cargo test --locked`、`cargo tree` の依存に C を compile する `build.rs` が無い |
| WP3 | CLI と `Backend`: `src/main.rs`・`src/driver.rs`（`Backend`・`BuildOptions::backend`・`validate`・`fast_backend_host`）、`F16Path::Soft` の指定、E2000 の単体テスト（`rejects_fast_backend_combinations`・`accepts_fast_backend_at_o0`） | `cargo test --locked --bin tsuzuri` と `--lib`。既定の IR・object が before と byte 同一 |
| WP4 | IR の front end（`src/backend/ir.rs`）: 走査・本体の解析・metadata と属性の読み飛ばし・資源上限（深さ・大きさ） | `parses_emitted_ir_for_fixtures`（define・declare・global の数が一致）、`src/runtime/*.ll` の 30 ファイルと numeric・math が解析できる |
| WP5 | layout と ABI の分類（`layout.rs`）。aarch64・x86_64 の DataLayout（`i128` は 16 byte） | `layout_matches_clang`（Clang で畳んだ定数と比較。別 arch は triple 指定） |
| WP6 | 翻訳の核（`lower.rs`・`mod.rs`）: scalar・制御・memory・集約（二層）・call と ABI・data と再配置・定数式・linkage・intrinsic・atomic・TLS・trap・`i128`・到達性・object の書き出しと後処理 | `rejects_unsupported_constructs_with_e2002`・`object_is_deterministic`・`lowers_every_runtime_ll`（numeric・math を含む） |
| WP7 | vector の lane scalar 化（`shufflevector`・`extractelement`・`insertelement`・`bitcast <N x i1> to iN`・`llvm.vector.reduce.*`）と残りの intrinsic | `string_v128.ll` を含む module と `fixtures/simd` が翻訳できる。vector の演算ごとの境界の test（独立の参照と比較） |
| WP8 | driver への組み込み: in-memory で翻訳、`module.o` を作って同じ link コマンド、`--emit object` の結合、並列、段、trap の signal の一致、`__fast-object` | `target/release/tsuzuri run examples/hello --backend fast` の stdout が `--backend llvm -O0` と一致、trap の fixture で E2005 と位置が一致、stderr が空、SHA-256 が 2 回・`TSUZURI_THREADS=1` で一致 |
| WP9 | ゲート: M1（合成 200・1,000 module の `build`・`build-edit-body`・`build-edit-signature`）と M2。fast が LLVM 以上なら停止条件 9 で止めて報告する | `target/perf/PB05/gate.txt` |
| WP10 | parity: `tests/fast_backend.mjs` の既定 mode（CLI・trap・cache・決定性・不変）と `parity` mode（経路 A・B）。翻訳の誤りの修正（期待値は変えない） | `npx --yes --package=node@24 node tests/fast_backend.mjs target/release/tsuzuri parity` が成功し、suite ごとの翻訳件数を表示 |
| WP11 | 生成コードの確認・M1〜M5 の記録・文書（`docs/architecture.md`・`docs/language.md`・`README.md`・`_tsuzuri/language-reference/compiler/` の `option.md`・`usage.md`・`debugging.md`・`platforms.md`・`docs/benchmarks.md` の PB05 の節） | `cargo fmt --check`、clippy（feature あり・なし）、`cargo test --locked`（あり・なし）、全 E2E、`node scripts/check-docs.mjs <変更したページ>` |
| WP12 | Phase 2: `-g` の DWARF の行情報（`src/backend/debug.rs`、gimli） | `llvm-dwarfdump --debug-line` で file:line の対応が LLVM 経路と一致、lldb で file:line の breakpoint が止まる（macOS は dsymutil 経由） |
| WP13 | Phase 2: `test` の `--backend fast`（`src/test_runner.rs` の 2 つの `clang -x ir` を置き換え） | `tests/test_runner.rs`・`tsuzuri test --backend fast` の fixture（失敗・trap・property） |
| WP14 | Phase 2: Windows の x86_64（COFF・`CallConv::WindowsFastcall`・inline の probestack・`windows_abi` の IR）。aarch64 の Windows は E2000 | `cargo check --all-targets --features fast-backend --target x86_64-pc-windows-msvc`（この機械で実行できるのは型検査まで。実行は `windows.yml`） |
| WP15 | Phase 2: `--allocator small\|counting`・`--trap-mode return`・`--freestanding`・`--emit shared` を parity の結果で許す、または E2000 のまま理由を文書化 | `allocator.mjs`・`small_allocator.mjs`・`trap_return*.mjs` の経路 B |
| WP16 | Phase 3: D13 の判断（WP9 の結果と、wasm の build の実測で再確認）。実装する場合は WP17〜20 | 判断の記録 |
| WP17〜20 | D13: wasm の object writer、本体の lowering、`wasm-ld` との結合と parity（wasm32 の経路 B）、文書と計測 | wasm の suite、`WebAssembly.validate`、import が空 |

### 受け入れ条件の変更

- Phase 1 の範囲は「option の組み合わせ」の表。`run`（trap-info と E2005 の位置を含む）と `--emit exe|object` が動き、範囲外は E2000・E2002。macOS・Linux の aarch64・x86_64 が対象だが、この機械で実行できるのは aarch64 の macOS だけ（別 arch は object の smoke と CI）。
- `--features` なしの `cargo build --release --locked`・`cargo test --locked`・clippy・fmt が従来どおり成功する。`--features fast-backend` でも成功する（`cargo +1.85.0 build --release --locked --features fast-backend` が成功する、という条件は満たせない。N3）。
- `--backend fast` を指定しないビルドの IR・object・実行ファイル（wasm を含む）が before と byte 同一（Mach-O の `LC_UUID`・署名は除く）。
- parity: 経路 A・B の両方が成功し、suite ごとの翻訳件数・fall back の件数と理由を `docs/benchmarks.md` の PB05 の節に書く。fall back は E2002 の構成要素（`half` など）だけで、suite の期待値・上限は変えない。`live == 0`。
- 追加: Phase 2（`-g` の行情報・`test`・Windows の型検査・allocator など）の各 WP の確認、D13 の判断の記録（実装した場合は WP17〜20 の確認）。D12 は行わない。
- 速さの数値は CI の合否にしない（共有機の参考値と、静かな機械での正式値を分ける）。

### 停止条件の更新

- 解消: 1（0.121.2 がある）、2（C を compile する `build.rs` は無い）、3（個別の方針を決めた）、4（境界の signature は scalar と ptr だけ。1,651 件で確認。新しい IR で崩れたら止める）、8（`LC_BUILD_VERSION` で消える。`tests/fast_backend.mjs` が stderr の空を検査）。
- 維持: 5（`--backend fast` の結果が LLVM 経路と一つでも異なる）、6（既定の出力が変わる）、7（`unsafe`）、9（合成 1,000 module の `build-edit-body` の中央値が LLVM 経路以上。人間の判断に回す）、10（既存テストの期待値を変える）。
- 追加: 11（ELF・COFF の TLS・atomic が CI のどれかの host で動かないとわかった: その構成要素を含む program は E2002 にして続け、報告に書く）、12（深い再帰の E2E が fast だけで SIGSEGV: stack の上限は上げず、
  `AGGREGATE_FLATTEN_LEAVES`・`regalloc_algorithm`・frame の使い方を調べる）、13（`--backend fast` を既定にしたくなった: D12 は別の承認）。

### 他チケットとの関係と衝突しやすい箇所

- 衝突しやすい: `src/main.rs`（`parse_arguments` の `BuildOptions { … }` と `HELP`。PB06 の `cli(raw, out, err)`、PR07・PR08 の option）、`src/driver.rs`（`BuildOptions`・`validate`・`build_complete` の link の組み立て。PR07 の `build_profiled`、PR08 の `Emit::Bitcode`）、
  `src/lib.rs`、`Cargo.toml`・`Cargo.lock`（crate を足すのは PB05 だけ）、`README.md`・`docs/architecture.md`・`docs/language.md`・`_tsuzuri/language-reference/compiler/option.md`・`usage.md`。新規の `src/backend/**`・`tests/fast_backend.*` は衝突しない。
- PM02（`%tz.closure` の縮小）・PM03（Latin-1 の文字列、`string_v128.ll` などの変更）・PM04（list の chunk）は IR の形を変える。translator は構成要素ごとに汎用に作り、統合後に parity を再実行する。PM02 の静的 descriptor は global の初期化子の関数 address（再配置）で、
  PM03 が runtime に vector を足しても lane scalar 化で通る。
- PR07（PGO）・PR08（LTO・bitcode）は Clang の引数と出力の種類を増やす。`--backend fast` との組み合わせは E2000 にする（`--profile-generate`・`--profile-use`・`--emit bitcode`）。PB06（常駐サーバー）は `TargetIsa` や解析済みの runtime を保持できる（将来）。PB07 は関数単位の object cache の候補。

### 測って決めること

- ゲート（WP9）: 合成 200・1,000 module の `build-edit-body` の中央値（9 標本）。fast が LLVM 経路より短いこと。IR の解析（`backend.scan`・`backend.compile`）が fast の total の 10% を超えないこと。
- `AGGREGATE_FLATTEN_LEAVES`（8・16・32）: 翻訳の時間と frame の大きさ（`stack_overflow.mjs` と深い再帰の E2E）。
- `regalloc_algorithm`（`backtracking`・`single_pass`）: compile の時間・code の大きさ・frame。
- 生成コードの実行時間（M3）と、deep recursion の frame（LLVM 経路との比）。
- M5: `x86`+`arm64` と host だけの binary の大きさ・build の時間・依存の行数。
- 機械で実行できない構成（x86_64・Linux・Windows）は、CI の job に載せる suite を報告に書く（`targets.yml` に parity、`windows.yml` に型検査と smoke）。
- 偵察では測っていないもの: IR の解析と翻訳の時間、fast の build の total（M1〜M3 は WP9）、`x86`+`arm64` の本物の binary の大きさと build の時間（M5 は WP2）、x86_64・Linux・Windows での実行（CI）。上の数値はどれも共有機の参考値である。

## 着手条件と停止条件

### 着手条件

- D1・D2・D3 が承認済みであること。承認前はどの手順にも着手しない（crate を追加しない試作もしない）。
- PB01 が done であること。PB05 は PB01 の `numeric`・`native-c` の事前ビルド object を使う（下記インターフェース）。
  確認: `grep -n "PB01\|PB02\|PX03" _perfs/README.md` の状態欄と、`grep -n "external_numeric\|fn runtime_object" src/llvm.rs src/driver.rs` が両方を見つけること。
- PB02 が done であること（依存欄のとおり。PB02 は目標 G3 の合算にだけ関わり、コードの前提ではない）。
- PX03 が done であること。手順 8 の計測は PX03 の生成器・variant・段別時間を使う（依存欄にはないが、計測の前提）。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 のベースラインを保存していること。

### 前提とする他チケットのインターフェース

- PB01: `llvm::EmitOptions::external_numeric: bool`（true なら `numeric.ll` を連結せず入口の宣言だけを出す）、
  `driver::runtime_object(cache, unit, source, input, args, directory, messages) -> Result<PathBuf, Diagnostic>`（unit は `"numeric"`・`"native-c"`）。
  PB05 の経路は常に `external_numeric = true` で IR を作り、この 2 つの object を link する（D7）。名前が違っていたら PB01 の実装に合わせ、意味が違えば止める。
- PX03: `TSUZURI_TIME_PASSES=1` で段別時間を出す仕組みと、`tool.` で始まる外部ツールの段名。PB05 は段 `backend.parse`・`backend.lower`・
  `backend.object`（新規）を足し、link の Clang は既存どおり `tool.` の段に入れる。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

1. `rust-version = "1.85"` で build できる Cranelift の版がない（D3 の選び方で見つからない）。`rust-version` を上げたくなった。
2. 追加する crate の依存木に、C/C++ を compile する `build.rs`、ネットワークを使う `build.rs`、既定で有効な `std` 以外の OS 依存 feature がある。
3. 手順 2 の棚卸しで、D6 の対応表にない命令・intrinsic・型・属性が出た。とくに `musttail`・`invoke`・`landingpad`・`va_arg`・
   `llvm.fmuladd`・`half`・`fp128`・`x86_fp80`・128 bit を超える vector、宣言だけの関数の境界での `byval`・`sret`・`inreg`。
4. 宣言だけの関数（C ランタイム、libc、PB01 の object、compiler-rt）の呼び出しに、集約値・`i128`・vector の引数か戻り値があり、
   Clang と同じ ABI になることを手順 6 のテストで確かめられない。
5. `--backend fast` の結果が LLVM 経路と一つでも異なる（stdout、stderr、終了コード、trap、`live`）。期待値や比較の条件を緩めない。
6. `--backend fast` を指定しないビルドの IR・object・実行ファイルの byte 列が HEAD（PB01 適用後）から変わる。
7. 自分の crate に `unsafe` が要る。`unsafe_code = "forbid"` を緩めない。
8. Cranelift の object を Clang で link すると、linker の warning かエラーが出て、Cranelift・`object` の設定で消せない。
9. 手順 8 の計測で、合成 1,000 モジュールの `build-edit-body` の中央値が LLVM 経路以上になった。CI の合否ではなく、Phase 1 を続けるかの人間の判断に回す。
10. 既存テストの期待値を変える必要がある。

## 現状と計測（HEAD `f8dc655`）

### ビルドの経路（コードで確認）

- `src/driver.rs` の `build` → `build_complete` は、`llvm::emit_with_options`（`EmitOptions { entry, wasm, debug_output }`）で 1 本の IR 文字列を作り、
  `TemporaryDirectory` の `module.ll` に書いて `tool("TSUZURI_CLANG", "clang")` に `-x ir` で渡す。WASM の link は `tool("TSUZURI_WASM_LD", "wasm-ld")`、
  macOS の `-g` は `tool("TSUZURI_DSYMUTIL", "dsymutil")`。
- `BuildOptions` は `target`・`emit`・`optimization`・`cpu`・`debug_output`・`trap_info`・`debug_info`・`wasm_simd`・`wasm_threads`・`cache`。
  `src/main.rs` は `optimization` を `test` だけ 0、ほかは 3 を既定にする。
- CLI は `--target native|wasm32`、`--emit exe|object|llvm|header|wasm|wgsl`、`-O0`〜`-O3`、`--cpu generic|native`、`-g`/`--debug-info`、
  `--trap-info`、`--debug-output`、`--wasm-feature simd128|threads`、`--no-cache`。`run` は `--target wasm32` を拒否する
  （`src/main.rs` のテスト `parse(&["run", "Main.tz", "--target", "wasm32"]).is_err()`）。編集→ビルド→実行の繰り返しは native だけで起きる。
- ランタイム: `src/llvm.rs` は使う記号があるときだけ `include_str!("runtime/<name>.ll")` を IR の末尾に連結する（`string`・`heap-native`・
  `heap-wasm`・`heap-wasm-threads`・`closure`・`recursive`・`numeric`・`math`・`character`・`console`・`debug`・`display`・`utf8string`・`task-wasm`・`wasm`）。
  手書きの `.ll` は `internal`（例: `string.ll` の `define internal %tz.string @tz.string.new`）で、`%tz.*` の型定義を利用者 IR のヘッダーに頼るので、
  単独では事前 compile できない。`numeric.ll`（`define` 21 個）と `math.ll`（61 個）は Clang の生成物。C の `task.c`・`cpu.c`・`io.c` は
  `src/driver.rs` が別に Clang で compile する。
- LLVM 以外の出力は `src/gpu.rs`（530 行）の WGSL だけで、`wgsl_type` などが対応外の形を診断で拒否する。
- LLVM への lowering は `src/llvm*.rs` の合計 10,888 行（うち `src/llvm.rs` 5,375 行）。型付き IR から第二のバックエンドを作ると、
  この量の所有・解放・trap・評価順序の規則を二重に持つ（D2 の理由）。
- 依存 crate は `rustc_apfloat`・`serde_json`（Windows だけ `same-file`）。`[lints.rust] unsafe_code = "forbid"`、`rust-version = "1.85"`、
  `edition = "2024"`、release は `lto = "thin"`・`codegen-units = 1`。
- `src/cache.rs` の `build_key` は `BuildOptions` と IR から鍵を作る。バックエンドを区別する入力はまだない。
- `tests/e2e.mjs` の trap の C harness は SIGILL と SIGTRAP の両方を `_Exit(86)` にする。native の trap 命令の種類（`brk`・`ud2`・`udf`）には依存しない。

### 計測済みの事実

`_perfs/README.md`（2026-09-29、Apple M1 Max、`--no-cache`、各 1 回）より。

| プロジェクト | 行数 | `check` | `--emit llvm` | 実行ファイル `-O0` | 実行ファイル `-O3` | RSS |
| --- | --- | --- | --- | --- | --- | --- |
| 合成 1,000 モジュール | 124,003 | 2.12 s | 2.58 s | 約 6.2 s | 約 10.1 s | 1.12 GB |

- Clang の処理は `-O0` で 3.70 s、`-O3` で 7.62 s。IR は約 29 MB。`-O0` で PB05 が削れる上限は Clang の 3.70 s から、
  翻訳・Cranelift・link の時間を引いた値である（IR の生成 2.58 s は減らない）。
- 小さなプログラムは Clang の起動とランタイムの compile が支配的で、その大半は PB01 の対象。

### 再現（2026-09-30 に確認）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
mkdir -p /tmp/tz-work-PB05
target/release/tsuzuri build examples/hello --emit llvm -O0 -o /tmp/tz-work-PB05/hello.ll
wc -c /tmp/tz-work-PB05/hello.ll
grep -ohE '^ +(%[-A-Za-z0-9._]+ = )?[a-z]+' /tmp/tz-work-PB05/hello.ll | sed -E 's/^ +(%[^ ]+ = )?//' | sort | uniq -c | sort -rn
grep -ohE '@llvm\.[a-z0-9_.]+' /tmp/tz-work-PB05/hello.ll | sort -u
/usr/bin/time -p target/release/tsuzuri build examples/hello -O0 --no-cache -o /tmp/tz-work-PB05/hello-O0
```

結果: IR は 3,126 bytes。命令は 14 種（`ret` 13、`call` 8、`br` 8、`load` 5、`insertvalue` 5、`extractvalue` 4、`icmp` 3、`store` 2、`phi` 2、
`alloca` 2、`sub` 1、`ptrtoint` 1、`inttoptr` 1、`add` 1）、intrinsic は `@llvm.trap` だけ。ビルドは `real 0.12`（1 回）。
調査メモにあった「hello の IR が 464 KB」は `examples/hello` では再現しない（`numeric.ll` は浮動小数点の表示などを使うときだけ連結される）。
この 2 つの `grep` が手順 2 の棚卸しの原型である。

## 目標と指標

目標は CI の合否条件にしない。専用の計測機で、同じ commit・同じ binary（`--features fast-backend` で build）の `--backend llvm` と `--backend fast` を比べる。

- G1: native `-O0` の編集→ビルド→実行の待ち時間（PX03 の `build-edit-body`）を短くする。
- G2: native `-O0` のコード生成の時間を、Clang の 3.70 s（合成 1,000 モジュール）より短くする。
- G3（長期。Phase 1 の受け入れ条件ではない）: PB02・PB04 と合わせて 124K 行の `-O0` を 1 s 以下にし、同じ規模の Go のビルド（PX03 Phase 2）より短くする。
- G4: 生成コードの実行時間の悪化を記録する。デバッグビルドとして許容できるかは人間が判断する（上限は決めない）。
- G5: 追加の費用（`tsuzuri` の大きさ、`cargo build --release --locked` の時間、依存 crate の数）を記録する。

| 指標 | 単位・統計 | 対象 | 期待（未検証。計測で確かめる） |
| --- | --- | --- | --- |
| M1 ビルド全体 | s（9 標本の中央値・最小・最大） | 合成 200・1,000 モジュールの `build`（`--no-cache`）と `build-edit-body`・`build-edit-signature`（cache あり）、`examples/hello` の `build` | fast が短い。上限の計算値は Clang の compile 分（1,000 モジュールで最大 3.70 s）の削減 |
| M2 段別 | s（中央値） | M1 の各標本の `TSUZURI_TIME_PASSES` の段。LLVM は `tool.clang`、fast は `backend.parse`・`backend.lower`・`backend.object` と link の `tool.clang` | どの段が支配的かを記録する |
| M3 生成コードの実行時間 | ms（9 回の中央値・最小・最大） | `examples/computations` と合成 1,000 モジュールの実行ファイル（stdin `12345\n`）を `-O0` で両経路 | fast が遅くなりうる。比を記録するだけ |
| M4 大きさ | bytes（固定値） | 合成 1,000 モジュールの `module.o` と実行ファイル | 記録するだけ |
| M5 費用 | bytes・s・個（各 1 回） | `target/release/tsuzuri` の大きさ、`cargo build --release --locked` の時間、`cargo tree -e normal --prefix none` の行数。feature の有無で | 記録するだけ |

## 変えてはいけない意味

判定の基準は「同じ IR を Clang `-O0` で compile した結果と観測できる差がない」こと。LLVM の poison・UB を具体的な値や trap にするのは、
Tsuzuri の IR がその経路に到達しない場合に限って許す（D6 の各行に理由を書く）。

- 整数: 幅ごとの wrap、shift、比較、`i128` の演算。`i128` の除算・剰余は LLVM と同じ compiler-rt の `__divti3`・`__udivti3`・`__modti3`・`__umodti3` を呼ぶ。
- 浮動小数点: binary32・binary64 の IEEE 754 の丸め（最近接偶数）、NaN の payload、符号付きゼロ。NaN の正規化をしない
  （Cranelift の `enable_nan_canonicalization` は false のまま）。FMA は IR の `llvm.fma` だけで、`fmul` と `fadd` を融合しない。
  fast-math の flag（`fast`・`nnan`・`ninf`・`nsz`・`arcp`・`contract`・`afn`・`reassoc`）が IR に現れたら E2002 で止める。
- 型変換: 整数どうしはビットを保つ。浮動小数点→整数は `llvm.fptosi.sat`・`llvm.fptoui.sat` の飽和（NaN は 0）。
- trap: `@llvm.trap` と `unreachable` の位置と、プロセスが受けるシグナル（D6。aarch64 の LLVM は SIGTRAP、x86_64 は SIGILL）。
  `run` の E2005 の表示と終了コードも LLVM 経路と同じにする。
- 評価順序・所有権・借用・解放: IR の命令の順をそのまま写す。命令の並べ替え、削除（`llvm.assume` などの D6 の列を除く）、重複をしない。
  すべての E2E で `live == 0`。
- ABI: 宣言だけの関数（C ランタイム、libc、PB01 の object）の呼び出しと、外から呼ばれる定義（`main`、`tz_*`）の引数・戻り値は、Clang と同じ C ABI。
- 決定性: 同じ IR から byte 単位で同じ `module.o` を作る。記号・data の順は IR に現れた順で、`HashMap` の反復順を使わない。
- LLVM 経路と WASM: `--backend fast` を指定しないビルドの IR・object・実行ファイルは byte 単位で変わらない。WASM の既定の import は増えない。
- 資源: 既存の資源上限（E1017）、`MAX_VALUE_BYTES` と診断を変えない。stack の使い方は変わりうる（落とし穴）。

## 設計

### 方式の比較（D1・D2）

| 観点 | (A) Cranelift ← `-O0` の LLVM IR テキスト（採用） | (B) Cranelift ← 型付き IR | (C) 直接 WASM ← 型付き IR | (D) 直接 WASM ← LLVM IR テキスト | (E) LLVM のまま（PB01・PB03・PB07） |
| --- | --- | --- | --- | --- | --- |
| 対応 target | native（aarch64・x86_64。Cranelift の公開情報、未検証） | 同左 | wasm32 だけ | wasm32 だけ | すべて |
| `run` の繰り返しに効くか | 効く（`run` は native だけ。確認済み） | 効く | 効かない（`run` は wasm32 を拒否。確認済み） | 効かない | 効く |
| 新しい crate | cranelift 系（D3）。依存木と大きさは未計測（手順 3 で計測） | 同左 | なし | なし | なし |
| `unsafe` | 依存 crate の内部だけ。自分の crate の `forbid` は保つ | 同左 | なし | なし | なし |
| ランタイム | 手書きの `.ll`・`math.ll` は module と一緒に翻訳。`numeric`・C は PB01 の object | lowering をもう一組書く | `.ll` は `internal` で `%tz.*` に依存し、単独で wasm32 object にできない（確認済み） | 再配置可能な wasm object（`linking`・`reloc.*` section）の生成と、ランタイムの linkage の変更が要る | 変更なし |
| 意味の一致 | 所有・trap・順序の lowering は 1 か所。命令の翻訳の誤りだけが危険 | 10,888 行の lowering を二重に持つ | 同左 | (A) と同じ利点 | 変化なし |
| デバッグ情報 | Phase 1 はなし。Phase 2 で行情報（`gimli`、要承認） | 同左 | WASM の DWARF は別途 | 同左 | あり |
| 既知の時間 | Clang `-O0` の 3.70 s を置き換える（計測済みは置き換え前だけ）。Cranelift の速さは未検証 | 同左 | WASM の `-O0` は未計測 | 同左 | Clang 3.70 s。PB07 の並列化の効果は未計測 |

(E) は PB05 と排他ではない。手順 8 で (A) が (E) より速いことを実測してから Phase 1 を完了とする（停止条件 9）。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| CLI | `src/main.rs` | オプションの解析（`--cpu` の分岐の隣）、`BuildOptions` を作る箇所 | `--backend llvm\|fast`（新規）。重複・未知の値・組み合わせを E2000（診断の表）。`--backend fast` で `-O` 省略なら 0 |
| 駆動 | `src/driver.rs` | `Backend`（新規）、`BuildOptions::backend`（新規）、`Default` | 既定は `Backend::Llvm` |
| 駆動 | `src/driver.rs` | `build_complete` | `Backend::Fast` なら `external_numeric = true` で IR を作り、`clang -x ir` の代わりに `backend::compile` で `module.o` を作る。link は `dwarf_sidecar` の経路と同じ「object を作ってから `clang` で link」の形で、`-lm`・`runtime_object`・`-pthread` を同じ条件で付ける。`--emit object` は既存の `-r -nostdlib` の結合をそのまま使う |
| 駆動 | `src/driver.rs` | 環境の検査（新規の小関数 `fast_backend_host`） | Windows と aarch64・x86_64 以外の host は E2000。feature なしの build では E2000 |
| cache | `src/cache.rs` | `build_key` | 変更なし。`options` の `Debug` 表示に `backend` が入るので鍵は自動で分かれる（手順 7 の E2E で確かめる） |
| crate | `src/lib.rs` | module 宣言 | `#[cfg(feature = "fast-backend")] pub mod backend;` |
| 翻訳 | `src/backend/ir.rs`（新規） | `parse`、`Module`・`Function`・`Inst`・`Ty`・`Constant` | Tsuzuri と runtime の IR の部分集合の構文解析 |
| 翻訳 | `src/backend/layout.rs`（新規） | `size_align` | host の既定 DataLayout の大きさ・align・field の offset |
| 翻訳 | `src/backend/lower.rs`（新規） | `lower_function` | D6 の表のとおり Cranelift IR を作る |
| 翻訳 | `src/backend/mod.rs`（新規） | `compile` | 記号の宣言、data、関数の定義、object の書き出し、段別時間 |
| テスト | `tests/fast_backend.rs`・`tests/fast_backend.mjs`（新規） | — | テスト計画 |

### データ構造

```rust
// src/driver.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend { Llvm, Fast } // （新規）
pub struct BuildOptions { /* 既存の field */ pub backend: Backend } // （新規 field）

// src/backend/mod.rs（新規）。triple は target_lexicon::Triple::host()
pub fn compile(ir: &str, object: &Path) -> Result<(), Diagnostic>;

// src/backend/ir.rs（新規）。名前は IR の文字列をそのまま持つ（`@` と `%` は除く）
pub enum Ty { Void, Int(u32), F32, F64, Ptr, Named(Box<str>), Struct { fields: Vec<Ty>, packed: bool }, Array(u64, Box<Ty>), Vector(u32, Box<Ty>) }
pub struct Module { pub types: Vec<(Box<str>, Ty)>, pub globals: Vec<Global>, pub declarations: Vec<Declaration>, pub functions: Vec<Function> }
pub struct Function { pub name: Box<str>, pub linkage: Linkage, pub params: Vec<(Ty, Box<str>)>, pub ret: Ty, pub blocks: Vec<Block> }
pub struct Block { pub label: Box<str>, pub insts: Vec<Inst> }
pub enum Linkage { External, Internal, Private, Hidden } // これ以外は E2002
// Inst は D6 の表の命令ごとに 1 variant。巨大な field は Box にする
```

- 集約値（struct・array）は SSA の中で「Cranelift の値の列」に平らにする（D8）。`insertvalue`・`extractvalue` は列の置き換えと取り出しで、命令を出さない。
- 集約の `load`・`store` は `layout.rs` の offset で field ごとの load・store にする。`i1` は Cranelift の `i8`（0 か 1）で表し、メモリでも 1 byte。
- 内部の関数（定義が同じ module にあり外から呼ばれない）は平らにした列をそのまま引数・戻り値にする。外との境界は D8。

### 翻訳規則（D6 の要約）

| LLVM | Cranelift | 理由・条件 |
| --- | --- | --- |
| `add`・`sub`・`mul`（`nsw`・`nuw` は捨てる）、`and`・`or`・`xor` | `iadd`・`isub`・`imul`・`band`・`bor`・`bxor` | wrap。poison の flag を捨てるのは refinement |
| `sdiv`・`udiv`・`srem`・`urem`（8〜64 bit） | 同名の命令 | Cranelift は 0 除算と `MIN / -1` で trap。LLVM では UB で、Tsuzuri の IR は事前の分岐で除外する（手順 6 で確認） |
| 同上（`i128`） | `__divti3` など compiler-rt の呼び出し | LLVM と同じ関数 |
| `shl`・`lshr`・`ashr` | `ishl`・`ushr`・`sshr` | 幅以上の shift は LLVM で poison。Tsuzuri の IR が事前に検査することを手順 2 で確かめる |
| `icmp`・`fcmp` | `icmp`・`fcmp`（述語を一対一。`fcmp true`・`false` は定数） | `one`・`ueq`・`ord`・`uno` も一対一の条件がある |
| `fadd`・`fsub`・`fmul`・`fdiv`・`fneg` | 同名の命令 | fast-math の flag は E2002 |
| `frem` | `fmod`・`fmodf` の呼び出し | LLVM も libm を呼ぶ |
| `zext`・`sext`・`trunc`・`fpext`・`fptrunc`・`sitofp`・`uitofp` | `uextend`・`sextend`・`ireduce`・`fpromote`・`fdemote`・`fcvt_from_sint`・`fcvt_from_uint` | — |
| `fptosi`・`fptoui`・`llvm.fpto[su]i.sat.*` | `fcvt_to_sint_sat`・`fcvt_to_uint_sat` | 範囲内は同じ値。範囲外は LLVM で poison なので飽和でよい。`fcvt_to_sint`（trap する）は使わない |
| `bitcast`・`ptrtoint`・`inttoptr`・`freeze` | `bitcast`・値の受け渡し | `freeze` は Cranelift に poison がないので恒等 |
| `select`・`phi`・`br`・`switch`・`ret` | `select`・block の引数・`brif`/`jump`・`cranelift_frontend::Switch`・`return` | — |
| `alloca` | `StackSlotData`（大きさと align は `layout.rs`） | — |
| `load`・`store`・`getelementptr` | `load`・`store`・`iadd`/`imul_imm` | `volatile`・`atomic` の load/store は E2002（手順 2 で出なければ実装しない） |
| `call`・`tail call` | `call`・`call_indirect` | `tail` は hint なので捨てる。`musttail` は停止条件 3 |
| `@llvm.trap`・`unreachable` | aarch64: `debugtrap` の後に `trap`、x86_64: `trap` | シグナルを LLVM と合わせる（SIGTRAP・SIGILL）。手順 6 で両経路の終了シグナルを比べる |
| `llvm.memcpy`・`llvm.memset` | `call_memcpy`・`call_memset`（libc） | `volatile` 引数が true なら E2002 |
| `llvm.fabs`・`llvm.copysign`・`llvm.sqrt`・`llvm.fma`・`llvm.abs`・`llvm.smax`・`llvm.smin`・`llvm.ctlz`・`llvm.scmp`・`llvm.is.fpclass` | `fabs`・`fcopysign`・`sqrt`・`fma`・`iabs`・`smax`・`smin`・`clz`・比較と減算・ビット検査 | 棚卸しで現れた型と flag の組だけを実装する |
| `llvm.assume`・`llvm.lifetime.*`・`llvm.experimental.noalias.scope.decl`・`!llvm.loop` などの metadata | 捨てる | 最適化の hint だけ |
| 128 bit 以下の vector | Cranelift の vector 型（`I32X4` など） | 128 bit を超える vector は E2002 |
| それ以外 | E2002 | 停止条件 3 |

### 生成コードとランタイム

- `backend::compile` は `cranelift_object::ObjectModule` で host の object（Mach-O・ELF）を 1 つ書く。`opt_level = "none"`、
  `enable_verifier` は debug build だけ true。関数の linkage は `Internal`・`Private` → `Local`、`External` → `Export`、`Hidden` → `Hidden`、宣言 → `Import`。
- data は `private`・`internal` の定数（文字列、集約の初期値、`zeroinitializer`）を `DataDescription` で出す。定数式の `getelementptr`・`ptrtoint` は
  `layout.rs` で offset を計算し、記号への参照は再配置で書く。
- ランタイム（D7）: 手書きの `.ll` と `math.ll` は IR に連結されたまま一緒に翻訳する。`numeric` は PB01 の `external_numeric` で宣言だけにし、
  PB01 の `runtime_object("numeric", …)` を link する。C ランタイムは `runtime_object("native-c", …)`。link は既存と同じ `tool("TSUZURI_CLANG", "clang")`。
- 段別時間は PX01 の仕組みで `backend.parse`・`backend.lower`・`backend.object` を記録する。

### 診断

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| E2000 | `--backend` の値が不正・重複 | `backend must be 'llvm' or 'fast'` / `backend specified more than once` | なし（CLI） |
| E2000 | `build`・`run` 以外に `--backend` | `--backend applies only to build and run` | なし |
| E2000 | `--backend fast` と `-O1`〜`-O3` | `--backend fast supports only -O0; remove -O{n} or use --backend llvm` | なし |
| E2000 | `--backend fast` と `--target wasm32`・`--wasm-feature` | `--backend fast supports only the native target; use --backend llvm for wasm32` | なし |
| E2000 | `--backend fast` と `--emit llvm\|header\|wasm\|wgsl` | `--backend fast produces only exe and object outputs; use --backend llvm for --emit {kind}` | なし |
| E2000 | `--backend fast` と `-g`・`--trap-info`・`--cpu native` | `--backend fast does not support {option} yet; remove it or use --backend llvm` | なし |
| E2000 | host が Windows か aarch64・x86_64 以外 | `--backend fast is not available on {os}-{arch}; use --backend llvm` | なし |
| E2000 | feature なしで build された `tsuzuri` | `this tsuzuri was built without the fast backend; rebuild with cargo build --release --features fast-backend or use --backend llvm` | なし |
| E2002 | 翻訳できない命令・型・属性・linkage | `the fast backend cannot translate {construct} in @{function}; build with --backend llvm and report this program` | なし |

### Phase 分割

- Phase 1（本チケットで実装）: macOS・Linux の aarch64・x86_64、`build --emit exe|object` と `run`、`-O0`、`-g`・`--trap-info`・`--cpu native` なし。
  cargo feature `fast-backend` の中だけ。差分 suite `tests/fast_backend.mjs` と計測まで。
- Phase 2（設計方針。人間が求めた場合だけ）: DWARF の行情報（Cranelift の source location と `gimli`、crate の追加は要承認）、`--trap-info`
  （trap 記号の表を Cranelift の trap 位置から作る）、`test` コマンド、Windows（G10 の後）。
- Phase 3（D13）: 直接の WASM 生成。WASM のデバッグビルドの待ち時間が問題になった場合だけ。
- 既定の選択（D12）: 本チケットの外。差分 suite が一定期間一致した記録を添えて、別の承認で判断する。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、`running N tests` の N を必ず見る（GUIDE §3.1）。
feature 付きのテストは `--features fast-backend` を付けないと 0 件になる。

### 手順 1: ベースライン

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行し、比較用の binary を保存する。「再現」のコマンドを実行して結果が同じことを確かめる。
- 確認: 次がすべて成功する。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked && cargo test --locked
mkdir -p target/perf/PB05 && cp target/release/tsuzuri target/perf/PB05/baseline-tsuzuri
{ git rev-parse HEAD; sysctl -n machdep.cpu.brand_string; sw_vers; clang --version | head -n 1; rustc --version; node --version; } > target/perf/PB05/env.txt
```

### 手順 2: IR の棚卸し（crate なし）

- 変更: `tests/fast_backend.mjs`（新規）の `inventory` mode。
- 内容: `tests/fixtures/*/` と `examples/*/` を一時ディレクトリに写し（E03 の再帰探索の落とし穴）、`build <dir> --emit llvm -O0` で IR を出す。
  IR から `src/runtime/numeric.ll` の本文を文字列置換で除き（fast 経路は PB01 の object を使う）、命令（「再現」の正規表現）、`@llvm.*` の名前、
  型（`half`・`fp128`・`<N x T>`・奇数幅の整数）、fast-math の flag、宣言の属性（`signext`・`zeroext`・`sret`・`byval`・`inreg`）、linkage を数えて、
  名前順の表を `target/perf/PB05/inventory.txt` に書く。build に失敗した dir は名前と stderr の 1 行目を表に書く。
- 確認: `node tests/fast_backend.mjs target/release/tsuzuri inventory` が成功する。表のすべての行が D6 の表にある。ない行があれば停止条件 3。

### 手順 3: crate と feature

- 変更: `Cargo.toml`、`Cargo.lock`、`src/lib.rs`、`src/backend/mod.rs`（新規、`compile` は常に E2002 を返す仮実装）。
- 内容: D3 のとおり optional な依存と `[features] fast-backend = [...]` を足す。既定の feature は空のまま。
- 確認: `cargo build --release --features fast-backend` で `Cargo.lock` を更新した後、`cargo build --release --locked`・`cargo build --release --locked --features fast-backend`・
  `cargo test --locked` が成功する。M5（大きさ、`time cargo build --release --locked --features fast-backend`、`cargo tree -e normal --prefix none --features fast-backend | wc -l`）を
  `target/perf/PB05/cost.txt` に書く。停止条件 1・2 を確かめる。

### 手順 4: CLI・`Backend`・E2000

- 変更: `src/main.rs`、`src/driver.rs` の `Backend`・`BuildOptions::backend`・`build_complete`・`fast_backend_host`（新規）、`tests/fast_backend.mjs` の既定 mode。
- 内容: 「診断」の E2000 をすべて実装する。`Backend::Fast` は `external_numeric = true` で IR を作り、`backend::compile` を呼ぶ（まだ E2002）。
  `src/main.rs` の `optimization` の既定は `action == Action::Test || backend == Backend::Fast` なら 0。
- 確認: `cargo test --locked --bin tsuzuri rejects_fast_backend_combinations` が `1 passed`。`cargo test --locked` が成功する。
  `target/release/tsuzuri build examples/hello --backend fast -O3` が E2000 で終わる。`--backend` なしの `examples/hello` の IR が手順 1 と byte 単位で同じ。

### 手順 5: 構文解析と layout

- 変更: `src/backend/ir.rs`・`layout.rs`（新規）、`tests/fast_backend.rs`（新規）。
- 内容: 手順 2 の表の構文をすべて読む。metadata（`!N = ...`、`, !llvm.loop !N`）、属性 group（`attributes #N`）、無視してよい属性
  （`noundef`・`nonnull`・`readonly`・`writeonly`・`noalias`・`local_unnamed_addr`・`unnamed_addr`・`nounwind`・`#N`）は読み飛ばす。
  意味を持つ属性（`signext`・`zeroext`・`sret`・`byval`・`inreg`・`volatile`・`atomic`）は保持する。浮動小数点定数は 16 進（`0x...`）と 10 進の両方を bit 単位で読む。
- 確認: `cargo test --locked --features fast-backend --test fast_backend` で `parses_emitted_ir_for_fixtures`・`layout_matches_clang` が成功する。

### 手順 6: 関数本体の翻訳

- 変更: `src/backend/lower.rs`（新規）、`src/backend/mod.rs`。
- 内容: D6 の表の順に、scalar → 制御 → memory・集約 → 呼び出し → intrinsic → trap を実装する。除算・shift・`fptosi` の各行について、
  Tsuzuri の IR が LLVM の UB に到達しないこと（検査の分岐が前にあること）を `src/llvm.rs` の生成箇所で確かめ、`lower.rs` の該当 arm に 1 行の注記を書く。
- 確認: `target/release/tsuzuri run examples/hello --backend fast` の stdout が `--backend llvm -O0` と一致する。`rejects_unsupported_constructs_with_e2002`・
  `object_is_deterministic` が成功する。trap の fixture（テスト計画）で両経路の終了シグナルと E2005 のメッセージが一致する。

### 手順 7: 境界の ABI・data・ランタイム

- 変更: `src/backend/mod.rs`・`lower.rs`、`src/driver.rs` の link。
- 内容: 宣言だけの関数と外から呼ばれる定義（`main`・`tz_*`）の signature を C ABI に合わせる（D8）。data と定数式、PB01 の 2 つの object の link、
  `--emit object` の `-r -nostdlib` の結合。
- 確認: `node tests/fast_backend.mjs target/release/tsuzuri` が成功する。`lowers_every_runtime_ll` が成功する。停止条件 4・8 を確かめる。

### 手順 8: 計測の関門

- 変更: なし。
- 内容: 「計測手順」の M1 を合成 1,000 モジュールの `build-edit-body` だけで測る。
- 確認: 中央値を `target/perf/PB05/gate.txt` に書く。fast が LLVM 以上なら停止条件 9 で止めて報告する。

### 手順 9: 差分 suite

- 変更: `tests/fast_backend.mjs` の `parity` mode。翻訳の誤りの修正。
- 内容: D11 の wrapper で「テスト計画」の suite を実行する。失敗は翻訳の誤りとして直す。期待値や suite は変えない（停止条件 5）。
- 確認: `npx --yes --package=node@24 node tests/fast_backend.mjs target/release/tsuzuri parity` が成功し、suite ごとに fast のビルドが 1 回以上あったと表示する。

### 手順 10: 生成コードの確認

- 変更: なし。
- 確認: 「生成コードの確認」のコマンドと pattern をすべて満たす。

### 手順 11: 計測・文書・最終確認

- 変更: `docs/benchmarks.md`、`docs/architecture.md`、`docs/language.md`、`README.md`、`_perfs/README.md`。
- 内容: 「計測手順」をすべて行い、「ドキュメント」を更新する。
- 確認: `cargo fmt --check`、`cargo clippy --all-targets --locked --features fast-backend -- -D warnings`、`cargo test --locked`、
  `cargo test --locked --features fast-backend`、README.md の全 E2E（`--features` なしの binary）、手順 9 が成功する。

## 計測手順

GUIDE §14 と PX03 の「計測手順」に従う。生データは PX01 形式の JSON Lines で `target/perf/PB05-<UTC 時刻>/`（コミットしない）に置く。

- binary: `cargo build --release --locked --features fast-backend` の 1 つ。LLVM 経路はそれを直接、fast 経路は D11 の wrapper
  （`node tests/fast_backend.mjs target/release/tsuzuri wrapper <dir>` が作る sh script）を compiler として PX03 の本計測に渡す。
  wrapper の起動は `/bin/sh` の `exec` だけで、Node を挟まない。
- 環境: 手順 1 の `env.txt` と同じ項目を run のディレクトリに書く。電源に接続し、ほかの重い処理を止める。
- M1・M2: PX03 の variant `build`（`--no-cache`、`-O0`）・`build-edit-body`・`build-edit-signature` を合成 200・1,000 モジュールで、warm-up 1 回と 9 標本。
  LLVM と fast を組ごとに交互に実行する。`examples/hello` の `build -O0 --no-cache` も同じ統計で測る。
- M3: 両経路の `-O0` の実行ファイルを 9 回ずつ交互に実行し、中央値・最小・最大を記録する（対象は計測対象の欄）。
- M4: `module.o` と実行ファイルの大きさ。M5: 手順 3 の `cost.txt`。
- 記録: docs/benchmarks.md に PB05 の節を足し、日付、計測機、commit、Clang・rustc・Node・Cranelift の版、M1〜M5 の表、生データの保存先、
  そろえられなかった条件（PB01 の cache の状態など）を書く。計測していない数値を書かない。

## 生成コードの確認

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
T=/tmp/tz-work-PB05
target/release/tsuzuri build examples/hello -O0 --emit object --backend llvm -o $T/hello-llvm.o
target/release/tsuzuri build examples/hello -O0 --emit object --backend fast -o $T/hello-fast.o
target/release/tsuzuri build examples/hello -O0 --emit object --backend fast -o $T/hello-fast2.o
shasum -a 256 $T/hello-fast.o $T/hello-fast2.o
/opt/homebrew/opt/llvm@21/bin/llvm-nm -g --defined-only $T/hello-llvm.o > $T/defined-llvm.txt
/opt/homebrew/opt/llvm@21/bin/llvm-nm -g --defined-only $T/hello-fast.o > $T/defined-fast.txt
/opt/homebrew/opt/llvm@21/bin/llvm-nm -u $T/hello-fast.o
/opt/homebrew/opt/llvm@21/bin/llvm-objdump -d $T/hello-fast.o | grep -E 'brk|udf|ud2'
```

- 2 つの fast の object の SHA-256 が同じ。
- 外から見える定義の記号名の集合が LLVM 経路と同じ（アドレスの列は除いて比べる）。
- 未定義の記号は LLVM 経路の未定義の記号に、`memcpy`・`memset`・`fmod`・`fmodf`・`__divti3`・`__udivti3`・`__modti3`・`__umodti3` を足した集合に含まれる。
- trap は aarch64 で `brk` の直後に `udf`、x86_64 で `ud2`。LLVM 経路の aarch64 は `brk #0x1`（2026-09-30 に `clang -x ir -O0` で確認。終了コード 133 = SIGTRAP）。
- `tsuzuri build` の stderr が空（linker の warning がない。停止条件 8）。

## テスト計画

### Rust テスト

`tests/fast_backend.rs`（新規、先頭に `#![cfg(feature = "fast-backend")]`）。IR は `tests/control.rs` と同じく `llvm::emit` で作る。

- `parses_emitted_ir_for_fixtures`: 手順 2 で build できた fixture の IR を `ir::parse` が読み、関数・宣言・global の数が IR の `define`・`declare`・`@... =` の行数と一致する。
- `layout_matches_clang`: `i1`・`i8`・`i32`・`i64`・`i128`・`float`・`double`・`ptr`・`%tz.string` 相当の `{ ptr, i64 }`・`{ i32, [2 x i128] }`・`{ i8, <4 x i32> }`・
  `[3 x { i8, i64 }]` について、`@sN = constant i64 ptrtoint (ptr getelementptr (T, ptr null, i32 1) to i64)` と field の offset を並べた IR を
  `clang -x ir -O1 -S -emit-llvm -o -` で定数に畳み、`layout::size_align` と比べる（独立の参照は Clang）。
- `rejects_unsupported_constructs_with_e2002`: `va_arg`、`fadd fast`、`<8 x float>`、`musttail call`、`load volatile`、`linkonce_odr` を含む IR が E2002 になり、
  メッセージが構文と関数名を含む。
- `object_is_deterministic`: 同じ IR を 2 回 `compile` した object が byte 単位で同じ。
- `lowers_every_runtime_ll`: 文字列・関数値・表示・再帰型・Math・Task を使う IR（`string`・`closure`・`display`・`recursive`・`math`・`character`・`utf8string`・`heap-native` が連結される）を
  `compile` が E2002 なしで処理する。
- `src/main.rs` の tests: `rejects_fast_backend_combinations`（新規。診断の表の E2000 の各行と、`test` への `--backend`）、
  `accepts_fast_backend_at_o0`（新規。`build`・`run` と `-O` 省略が `optimization == 0` になる）。

### E2E

`tests/fast_backend.mjs`（新規）。既定 mode は次を確かめる。

- CLI: 診断の表の E2000 の各行が非 0 で終わり、stderr がメッセージを含む。
- `examples/hello`: `run --backend fast` と `run --backend llvm -O0` の stdout・終了コードが同じ。
- trap: `tests/fixtures/fast_backend/trap/Main.tz`（新規。`main` が 0 除算で trap する。式は `tests/e2e.mjs` の `divide` の fixture と同じ形）。
  先に LLVM 経路で `run` が E2005 になることを確かめ、fast の stderr（`program terminated with signal: ...` を含む）と終了コードが一致する。
- 決定性: `--no-cache` の fast の実行ファイルを 2 回作り、SHA-256 が同じ。
- cache: cache ありで llvm → fast → llvm の順に build し、1 回目と 3 回目の成果物が同じ、fast の成果物は異なる。
- 不変: `--backend llvm` と指定なしの `--no-cache -O0` の成果物が byte 単位で同じ。

`parity` mode は D11 の wrapper で次の suite を実行する: `e2e`・`primitives`・`strings`・`tasks`・`computations`・`control`・`numeric_casts`・
`integer_intrinsics`・`display_parse`・`features`・`math`・`io`・`host_imports`・`gpu`。各 suite で fast のビルドが 1 回以上あることを wrapper の log で確かめる。
0 回の suite は一覧から外し、理由を docs/benchmarks.md の PB05 の節に書く。除外: `simd`・`wasm_simd`・`cpu_dispatch`（機械語の pattern を検査する）、
`debug_info`（`-g`）、`cache`（鍵の検査は既定 mode）、`wasm_threads`・`windows`・`lsp_sessions`・`docgen`・`examples`（`-O0` の native ビルドがない、または build しない）。
期待値は各 suite の独立の参照（JavaScript の BigInt、C の harness）のままで、`live == 0`、trap の `_Exit(86)` を含む。

### 既存テストへの影響

なし。`BuildOptions` の `Debug` 表示が変わるので利用者の cache は一度 miss になる（`compiler-binary` の digest でも同じことが起きる）。

### 性能

「計測手順」だけ。CI に速さの合否を入れない。

## ドキュメント

- `docs/architecture.md`: pipeline に `--backend fast` の経路（`-O0` の IR → `src/backend/` → Cranelift の object → Clang で link）と module 表の `src/backend/` を足す。
  「LLVM の C API に結合しない」方針が保たれていることを書く。
- `docs/language.md`: ビルド専用オプション（E2000 の段落）に `--backend llvm|fast` と Phase 1 の制限を足す。
- `docs/benchmarks.md`: PB05 の節（計測手順の記録）。
- `README.md`: `cargo build --release --features fast-backend` と `node tests/fast_backend.mjs` の 2 mode を検証コマンドに足す。
- `_perfs/README.md`: PB05 の状態と計測値。

## 受け入れ条件

- [x] D1・D2・D3 が承認されている（2026-10-17）。
- [x] `--backend fast` が Phase 1 の範囲で動き、範囲外は診断の表のとおり E2000・E2002 になる（Phase 2・D13 の範囲まで広げた）。
- [x] `--backend fast` を指定しないビルドの IR・成果物が byte 単位で変わらない（統合 branch の compiler との 576 件の比較で差 0）。`--features` なしの build と全テストが成功する（111 target、1,603 件）。
- [x] `tests/fast_backend.rs` と `tests/fast_backend.mjs` の既定 mode・`parity` mode が成功する。
- [x] 「生成コードの確認」をすべて満たす（hello の 2 つの object の SHA-256 が同じ、定義する symbol が同じ、未定義の symbol は LLVM 経路の部分集合、trap は `brk`、stderr が空）。
- [x] M1〜M5 を計測して docs/benchmarks.md に記録し、計測していない数値を書いていない（標本は 7 件。共有機の参考値）。
- [x] GUIDE §10 の完了の定義を満たす（静かな機械での正式な比較は統合担当）。

## 落とし穴

- Cranelift の `sdiv`・`udiv`・`srem`・`urem` は 0 除算と `MIN / -1` で trap し、shift 量は幅で mask する。LLVM の UB の経路に Tsuzuri の IR が到達しないことを
  手順 6 で生成箇所ごとに確かめる。到達するなら翻訳で合わせず停止条件 5。
- `fptosi` を `fcvt_to_sint` にすると、Clang の生成した `math.ll` が範囲外の値を先に変換して `select` で捨てる箇所で trap する。必ず `_sat` を使う。
- Apple の aarch64 の C ABI は `i8`・`i16` の引数を呼び出し側で拡張する。`signext`・`zeroext` を `AbiParam::sext`・`uext` に写さないと、C ランタイムが上位 bit のごみを読む。
- Cranelift は 2 語を超える戻り値に暗黙の戻り値領域を使い、LLVM の first-class aggregate の戻り値と ABI が違う。module の中だけなら問題ないが、
  宣言だけの関数と `tz_*` では停止条件 4。
- `@llvm.trap` を Cranelift の `trap` だけにすると aarch64 で SIGILL になり、`run` の E2005 の表示（`signal: 5 (SIGTRAP)`）が変わる。
- IR に `target triple`・`target datalayout` はない（Clang が host の既定を使う）。`layout.rs` は host の既定と一致させ、`layout_matches_clang` で確かめる。
- `!llvm.loop` の metadata は `-g` なしでも `br` に付く（`llvm.loop.unroll.enable`）。読み飛ばしを忘れると構文解析が失敗する。
- PB01 の `external_numeric` の条件は `options.cache` を含むが、fast 経路は常に true にする。`--no-cache` では `runtime_object(None, …)` が毎回 Clang で
  `numeric` を compile するので、M1 の `build`（`--no-cache`）はその時間を含む。PX03 の `build-edit-body` と分けて読む。
- `--emit object` の `-r -nostdlib` の結合は macOS で `-Wl,-keep_private_externs` が要る（GUIDE §11.1 の既知の落とし穴）。Cranelift の `Hidden` の記号で確かめる。
- stack の使い方は Cranelift と LLVM で違う。深い再帰の E2E が fast だけで SIGSEGV になったら停止条件 5（stack の上限を上げない）。
- `Cargo.lock` の更新前は `--locked` が失敗する。feature なしの `cargo test --locked` では `tests/fast_backend.rs` が 0 件になる。
- 重い BigInt の suite は Node 20 で V8 が落ちることがある。`parity` は Node 24 で実行する。

## 対象外

- `-O1` 以上、リリースビルド、LLVM と同等の最適化。
- WASM（D13）、Windows、`-g` の DWARF・`--trap-info`・`test` コマンド（Phase 2）。
- 既定のバックエンドの変更（D12）。REPL の JIT（G13）。LLVM の C API・Rust バインディングへの結合。

## 決定事項

### D1: 方式

- 決定: 第二のバックエンドは Cranelift で native の object を作る。直接の WASM 生成（(C)・(D)）と自前の機械語生成は採らない。
- 理由: 計測済みの待ち時間は native `-O0` の Clang（3.70 s）で、`run` は native だけ。WASM の直接生成は `run` に効かず、ランタイムの `.ll` が
  `internal` で `%tz.*` に依存するため wasm32 object に事前 compile できない。自前の機械語生成は aarch64・x86_64 の命令選択とレジスタ割り当てを抱える。
- 状態: 承認済み（2026-10-17）・実装済み（native は Cranelift。WASM の直接生成は D13 として別に実装した）

### D2: 入力

- 決定: Tsuzuri が出す `-O0` の LLVM IR テキスト（連結したランタイムを含む）を構文解析して翻訳する。`src/llvm*.rs` は変えない（PB01 の `external_numeric` を除く）。
- 理由: 10,888 行の lowering を二重に持たず、所有・trap・評価順序の一致を構成で保つ。IR の生成（2.58 s に含まれる）は残るが、Clang の compile 分を置き換える。
- 見直し提案: 起票時の目的は「型付き IR から直接」だった。IR テキストの生成と構文解析が支配的になったら、Phase 2 以降で `src/llvm.rs` の出力を構造化する案を別チケットで検討する。
- 状態: 承認済み（2026-10-17）・実装済み（IR テキストを in-memory で読む。`src/llvm*.rs` は変えていない）

### D3: crate と feature

- 決定: optional の依存として `cranelift-codegen`・`cranelift-frontend`・`cranelift-module`・`cranelift-object`・`target-lexicon` を足し、
  cargo feature `fast-backend`（新規）だけで有効にする。cranelift の 4 crate は同じ版を `=` で固定し、`cargo info cranelift-codegen@<版>` の
  `rust-version` が 1.85 以下の最新版を選ぶ。`target-lexicon` は cranelift が要求する版に合わせる。
- 理由: 既定の build 時間・binary の大きさ・依存を変えずに評価できる。依存 crate の内部の `unsafe` は自分の crate の `forbid` に触れない。
- 状態: 承認済み（2026-10-17）・実装済み（Cranelift 系 4 crate `=0.121.2`、`target-lexicon` 0.13、Phase 2 の `gimli` `=0.31.1`）

### D4: CLI

- 決定: `--backend llvm|fast`（新規）。既定は `llvm`。`build`・`run` だけが受ける。`--backend fast` で `-O` を省略すると `-O0`。
- 理由: 明示の選択だけにし、既存のコマンドの意味を変えない。
- 状態: 実装済み（`build`・`run`・`test` が受ける。2026-10-17 の再検討で `test` を足した。許す組み合わせは option.md の表）

### D5: Phase 1 の範囲

- 決定: 「Phase 分割」の Phase 1 のとおり。範囲外の option は E2000、翻訳できない IR は E2002 で、LLVM 経路への黙った fallback をしない。
- 理由: 明示した backend が使えないときは報告する（AGENTS.md）。fallback は計測と差分テストを曖昧にする。
- 状態: 実装済み（範囲は再検討で広げた: 全フェーズ。『実装と検証』の範囲）

### D6: 翻訳規則

- 決定: 「翻訳規則」の表のとおり。表にない構文は E2002 で、手順 2 で見つかったものは停止条件 3。
- 理由: 観測できる差がない写し方だけを選んだ（`_sat`、trap のシグナル、compiler-rt の `i128` 除算、NaN 正規化なし）。
- 状態: 実装済み（棚卸しで足した構成要素を含む。表にない構文は E2002）

### D7: ランタイム

- 決定: 手書きの `.ll` と `math.ll` は IR と一緒に翻訳する。`numeric` と C ランタイムは PB01 の object を link する。
- 理由: `.ll` は `internal` で型を利用者 IR に頼るので分けられない。`numeric.ll` は `sret`・`byval`・`fastcc` を内部で使い（確認済み）、翻訳の対象を増やすだけで得がない。
- 状態: 実装済み（再検討で変更: fast は LLVM 経路に渡る IR と同じ text を翻訳し、runtime の object は同じ関数・同じ引数で作る）

### D8: 集約値と ABI

- 決定: SSA の集約値は scalar の列に平らにする。module 内の定義は列をそのまま引数・戻り値にする。宣言だけの関数と外から呼ばれる定義は、
  scalar・`ptr` と、2 語以下の集約の戻り値だけを受け、`signext`・`zeroext` を写す。それ以外は停止条件 4。
- 理由: 棚卸しの範囲では境界の signature は scalar が中心で、aggregate の C ABI を一般に実装する必要がない。
- 状態: 実装済み（再検討で二層に変更: leaf が 16 以下は scalar の列、超えると frame 上）

### D9: デバッグ情報

- 決定: Phase 1 は DWARF を出さず、`-g` を E2000 にする。Phase 2 で行情報を足す場合は `gimli` の追加として別に承認を得る。G16 のデバッガー体験は LLVM 経路のまま。
- 理由: 行情報なしで `-g` を受けると、利用者が期待するデバッグができないのに成功に見える。
- 状態: 実装済み（Phase 2: 行情報だけの DWARF。変数と型は `llvm` だけ。`gimli` の追加は 2026-10-17 に承認済み）

### D10: cache

- 決定: `build_key` は変えず、`BuildOptions` の `Debug` 表示に入る `backend` で鍵を分ける。E2E で llvm と fast の成果物が混ざらないことを確かめる。
- 理由: 既存の鍵の作り方がすでに option 全体を含む。
- 状態: 実装済み（`build_key` は変えず、test で 2 つの backend の成果物が混ざらないことを確認）

### D11: 意味の一致の検証

- 決定: `tests/fast_backend.mjs` が生成する `/bin/sh` の wrapper を compiler として既存 suite に渡す。wrapper は第 1 引数が `build`・`run` で `-O0` を含み、
  `-g`・`--debug-info`・`--trap-info`・`--wasm-feature`・`--backend`・`wasm32`・`llvm`・`header`・`wasm`・`wgsl`・`native` のどれも含まないときだけ
  `--backend fast` を足して log に 1 行書き、`exec` する。suite の source は変えない。
- 理由: 既存の独立の参照と harness をそのまま使え、suite ごとの改修が要らない。除外の判定は保守的（`native` を含むと `--target native` も除く）。
- 状態: 実装済み（経路 A・B と `programs`・`wasm`・`debug`・`parity` の mode。wasm32 の wrapper を足した）

### D12: 既定の選択

- 決定: 本チケットでは既定を変えず、自動選択もしない。差分 suite が一定期間一致した記録を添えて、別の承認で判断する。
- 理由: 意味の一致が長期に確認されるまで、利用者の成果物を変えない。
- 状態: 未実施（本チケットの範囲外。`llvm` が既定のまま。別の承認と、静かな機械での測定・長期の差分の記録が要る）

### D13: 直接の WASM 生成

- 決定: 本チケットでは行わない。将来行う場合は、再配置可能な wasm object（`linking`・`reloc.*` section）を書き、ランタイムの `.ll` の linkage を変えて
  wasm32 object に事前 compile し、`wasm-ld` で link する設計を別チケットで承認を得て進める。
- 理由: WASM のビルドは `run` の繰り返しになく、ランタイムの linkage の変更は LLVM 経路の IR を変える。
- 状態: 既定案（実装者はこの案に従う）。**2026-10-17 の依頼と WP16 の判断で、直接の WASM 生成を実装した**（下の「WP16 の判断」）。LLVM 経路の IR は変えていない。

## 実装と検証（2026-10-18）

### 実装した範囲

全フェーズを実装した（D1・D2・D3・D9・D13 は 2026-10-17 の依頼で承認済み）。**D12（既定の切り替え）は行っていない**: 既定は `llvm` のままで、`--backend` を付けないビルドの出力は変わらない（旗なしの IR は `tests/fast_backend.mjs` の既定 mode が `--backend llvm` と byte 比較し、保存した基準の compiler とも `TSUZURI_BASELINE` で比べられる）。
基準は `a2313d19`（main `9dd0fbbc` + 第4フェーズの足場）、branch `perf/p4-PB05`。merge を除く 32 commit と、この文書の commit（下の表）。最後の統合 merge は `37a28528`（統合 branch `perf/phase4-compiler-performance` の `63a12d4a`）。

| commit | 内容 |
| --- | --- |
| `359550e1` | WP0 着手時の再検討（文書だけ） |
| `35be7e49` | WP2 cargo feature `fast-backend`（Cranelift 0.121.2 を `=` で固定。feature なしのビルドは変わらない） |
| `f80e0675` | WP3 `--backend llvm\|fast`、`BuildOptions::validate_fast`（allow-list。`BuildOptions` の全欄を destructure するので、欄を足すと決めるまで compile が通らない） |
| `369af842`・`ee9a0125` | WP4〜7 IR の読み取りと翻訳（layout・ABI・lowering・intrinsic・global data・object）。vector は lane に分ける |
| `3a0174f0` | WP8 driver: module の IR を in-process で翻訳して `module.o` を作り、同じ link コマンドを使う。`tz-fast-cc`（feature つきの test 用 helper binary）と route A・B の wrapper |
| `c16cba7c` | WP10 Clang との照合の test（`tests/fast_backend.rs`・`tests/fast_backend_ops.rs`）と、それが見つけた誤りの修正 |
| `394b16f3` | WP13 `tsuzuri test --backend fast` |
| `4fe000ef`・`b6ea5d87` | WP9・WP11 ゲートの計測 script（`benchmarks/run-backend.mjs`）、文書、常駐サーバーの test への組み込み |
| `77bb5582`・`34ea4b2e` | WP12 `-g` の DWARF（`!dbg` の位置を Cranelift の source location に、gimli で行表と関数の DIE） |
| `127a6e75` | WP15 `--allocator small\|counting`・`--freestanding`・`--trap-mode return` |
| `506d5c9f`〜`9250638a`（WP17 の 12 commit）、`3858bdff`（WP18）、`62799906`（WP20） | 直接の wasm32 生成（D13）: object writer、lowering、driver、差分テストと wrapper、locals の共有 |
| `315b5808`・`c16dbc7a`・`fbdc3702` | WP14 の確認で見つけた x86-64 の誤り（`i128` の署名、`fmuladd`）、別の機械向けの object の test と操作の書き出し、reducible でない loop の case |
| `4284f393`・`467e0149`・`3d47c701`・`92402c32`・`e0a98fd6` | wasm32 の前向きの分岐、`benchmarks/run-backend-kernels.mjs`、文書と測定の記録、clippy 1.99 の指摘、feature なしの build の test と README の `--backend` |
| 統合の merge | `4b315242`・`2beef865`・`7f0071f2`（PR07・PR08・PB06 など）、`e0508729`（`e4025531`）、`ce980025`（`0df672e7`。PM04 など）、`37a28528`（`63a12d4a`。PM04 の review fix、CI の配線） |

### 作業ごとの結果

| WP | 結果 |
| --- | --- |
| WP1 | `tests/fast_backend.mjs` の `inventory` mode: 102 project の IR と runtime の `.ll` に 51 種の命令・47 種の intrinsic・23 種の vector 型があり、D6 に無い構成要素の方針（翻訳する・E2002 で拒む）を表にした |
| WP2 | feature と crate（`cranelift-codegen`・`frontend`・`module`・`object` の `=0.121.2`、`target-lexicon` 0.13、WP12 の `gimli` `=0.31.1`）。rustc 1.93.1・1.98.1・1.99.0 で build が通り、依存は本物の rustc 1.85.0 でも build できる（`tsuzuri` 自身は 1.85.0 では build できない。base が let chain を使うため。N3）。依存に C を compile する `build.rs` は無い。Cranelift は 0.121.2 が `rust-version` 1.85 以下の最新（0.122 は 1.86 を要する） |
| WP3 | CLI。範囲外の組み合わせは build に入る前に E2000（`--cpu native`、`-O1` 以上、`--emit` の `llvm`・`header`・`shared`・`bitcode` など、`--profile-*`、`--target TRIPLE`、Windows のホスト、feature なしの build） |
| WP4〜7 | 翻訳。`module::scan`（型・global・declare・define の header と本体の範囲）、worker での本体の parse、到達できる関数だけを翻訳（D16）、集約は 16 leaf まで scalar（`FLATTEN_LEAVES`）、超えると frame 上、`i128`・atomic・TLS・weak・`fastcc`・`byval`・`sret`・overflow と saturation の intrinsic・vector の lane 分けを実装 |
| WP8 | driver。object は 1 つの `Vec<u8>`（`module.ll` は書かない）、翻訳は関数ごとに `TSUZURI_THREADS` の worker で並列で、object のバイト列は thread 数に依らず同じ。route B の helper は、隠し subcommand ではなく、feature つきの `[[bin]] tz-fast-cc`（`required-features = ["fast-backend"]`）にした |
| WP9 | ゲート: 通った（docs/benchmarks.md の PB05 の節）。native 0.50〜0.57 倍、wasm32 0.36〜0.47 倍。停止条件 9 は発動しなかった |
| WP10 | parity: route A（`tsuzuri` の wrapper）と route B（`clang` の wrapper）。fall back は `half` だけ（driver は `F16Path::Soft` を渡すので、`fast` のビルドでは出ない） |
| WP11 | 文書と測定の記録 |
| WP12 | `-g`: `.debug_line` の行表と関数ごとの `DW_TAG_subprogram`（DWARF 4）。`llvm-dwarfdump --verify` が通り、21 個の実行ファイルの 1,393 関数が `llvm` の行をすべて持つ。変数と型の情報は書かない（`llvm` だけ）。Linux の x86-64・AArch64 の ELF の object も `--verify` が通る。macOS で C の runtime が要る `--emit object -g` は E2002（`ld -r` が DWARF を捨てる） |
| WP13 | `test` の 2 つの `clang -x ir` を置き換えた。`--index N -g -o PATH` も同じ runner |
| WP14 | **Windows は、ホストでは E2000 のまま**（型検査だけで実行できないため）。翻訳器の x86-64 COFF の書き出し（`CallConv::WindowsFastcall`・inline の probestack・`tls_model = coff`）は 2 つの MSVC target で型検査を通る。確認で見つけたこと: (1) Cranelift の x86-64 は署名に `i128` があると panic する（`enable_llvm_abi_extensions` で直した。AArch64 の機械では出ない）、(2) aarch64 の Windows は `cranelift-object` が panic するので E2002 にした、(3) Windows 向けの IR は `windows_abi` が `__multi3` などを `weak hidden` の `i128` の署名で定義し、他の object が LLVM の libcall の規約で呼ぶので、翻訳器は E2002 で拒む（Windows で規約を確かめるまで有効にしない）。test `objects_for_other_machines_have_their_formats` が 5 つの target の object の形式を確かめる |
| WP15 | `--allocator small\|counting`・`--freestanding`・`--trap-mode return` は parity で一致し、許した。`--emit shared`・`--cpu native` は E2000 のまま（理由は文書） |
| WP16 | D13 を実装すると判断（下） |
| WP17〜20 | 直接の wasm32 生成。再配置可能な wasm object（`linking`・`reloc.CODE`・`reloc.DATA`・`target_features`）を自前で書き、`wasm-ld` でリンクする。`WebAssembly.validate` が通り、import は Clang の object と同じ順序、export も同じ |

### 実装しなかったもの

- D12（既定の切り替え）。`llvm` が既定のまま。
- 範囲外で E2000: `--cpu native`、`-O1` 以上・`-Os`・`-Oz`、`--emit shared|bitcode|llvm|header`、`--profile-*`、`--target TRIPLE`、Windows のホスト、wasm32 の `--wasm-feature`（simd128・threads・jspi・webgpu・net）・`--wasm-host wasi`・`--wasm-opt`・`-g`、wasm64。
- 翻訳器が E2002 で拒むもの: `half`（driver は `F16Path::Soft` なので実際には出ない）、128 bit の `smul.with.overflow`・`umul.with.overflow`、wasm32 の `llvm.fma`（命令が無い）、表にない `llvm.*`（`llvm.round`・`llvm.minnum`・`llvm.maxnum` など）、署名に `i128` を持つ外部の関数（native。Windows の `windows_abi` の helper）、外から見える関数の `byval`・`sret` の引数とその呼び出し、`musttail`、wasm32 の 1〜16・32・64・128 bit 以外の幅の整数の load と store、基本ブロックが 65,521 個を超える関数と local が 40,000 個を超える関数（wasm の engine の上限。local は値の数ではなく、使い回したあとの数）、aarch64 の Windows。拒むたびに構成要素と関数の名前を表示する。
- `AGGREGATE_FLATTEN_LEAVES`（16）と Cranelift の `regalloc_algorithm`（既定）の調整: ゲートを通ったので行わなかった（翻訳器は native の `fast` の total の 9〜12%）。
- wasm32 の loop は、前向きの分岐は構造化した `br` で、後ろ向きの分岐は dispatch の loop（`pc` の local と `br_table`）を通る。配列と list の loop は `llvm` の `-O0` の 1.3〜1.5 倍の時間になる（構造化した loop への変換は行っていない）。
- 速度の正式な測定（静かな機械）、Windows での実行、Intel の macOS での実行。

### 決定事項の追記

- D1・D2: 維持。入力は `-O0` の LLVM IR テキスト、in-memory、`src/llvm*.rs` は変えていない（native も wasm32 も、LLVM 経路の IR は byte 同一）。
- D3: crate は上のとおり。`object` は `cranelift_object::object` の再 export を使い、直接の依存は足していない。`cranelift-native` は使わない（出力が機械に依らない）。feature なしの build は従来どおり。
- D4・D5: `--backend llvm|fast`、`build`・`run`・`test`。範囲は option.md の表のとおり。
- D6: 表にない構文は E2002。vector は lane に分け、`llvm.vector.reduce.*`・`shufflevector`・mask vector を含む。
- D7: fast は LLVM 経路に渡る IR と同じ text を翻訳し、runtime の object は同じ関数・同じ引数で作る（cache は共有される）。
- D8: 二層の集約値。leaf が 16 以下は scalar の列、超えると frame 上。
- D9: 行情報だけ（WP12）。
- D10: `backend` は `build_key` に入る（`BuildOptions` の全欄が鍵になる）。test「the cache keeps the backends apart」が、2 つの backend の成果物を取り違えないことを確かめる。
- D11: wrapper は経路 A（`tsuzuri build\|run` に `--backend fast` を足す）と経路 B（`clang -x ir` の呼び出しを `tz-fast-cc` に）。経路 B は E2002 になったときだけ clang に戻り、件数と理由を数えて表示する。
- D12: 行わない。
- D13: 実装した（下）。D14〜D16（並列と決定性、Cranelift の設定、翻訳の単位）は計画のとおり。
- 追加（D17）: 翻訳器は host の CPU を検出せず、x86-64 は baseline とする。このため `llvm.fmuladd` は x86-64 で乗算と加算に分け（Clang が融合しない構成に合わせる）、AArch64 では融合する。`llvm.minimum`・`llvm.maximum` の NaN の payload と符号は、Clang と一致しないことがある（言語は NaN の伝播だけを約束する）。

### WP16 の判断（D13）

- 証拠: wasm32 の `-O0` の `llvm` のビルドは、Clang と `wasm-ld` が total の 55〜70%（200 module で 61〜70%、1,000 module で 55〜59%。docs/benchmarks.md の測定 1）を占め、native の Clang（48〜63%）と同じ規模である。wasm 用の runtime IR は vector と threads の atomic 以外の特別な命令を使わず、native の前半（解析・layout・到達性・集約の分類・intrinsic の表）をそのまま共有できる。
- 判断: **実装する**。実装して、200 module で 0.36〜0.42 倍、1,000 module で 0.45〜0.47 倍になった（`llvm` のビルド比）。止める理由になる blocker は無かった。
- 実装は計画の素案から変えた点がある: (1) 制御構造は dispatch の `loop`＋`br_table` のままで、前向きの分岐は構造化した `br`（`pc` を通らない）。(2) SSA 値は local で、生存期間が重ならない値は local を共有する（深い再帰の関数の local が 2,031 から 12 に）。(3) object writer・lowering・`i128`・vector・intrinsic・`wasm-ld` が読む再配置を自前で書いた（新しい crate は足していない）。(4) import は Clang と同じく最初に使われた順に番号を付ける（`tests/async.mjs` がその順序を観測する）。規模は計画の 3,500〜4,500 行に対して、実装は 約 6,300 行（`src/backend/wasm/`）だった。
- 結果: module は `llvm` の `-O0` より大きい（合成プロジェクトで 1.40〜1.43 倍、`examples/` と `tests/fixtures/` の 94 個の合計で 1.10 倍）。V8 での実行時間は、scalar の loop が `llvm` の `-O0` と同じ、配列と list の loop が 1.3〜1.5 倍、全体で 1.05 倍（`-O3` の 3.1 倍）。

### 確認

統合後の木（統合 branch の `63a12d4a` を merge した `37a28528` 以降）で確かめた。速度を除いて、すべての確認を実行した。

| 確認 | 結果 |
| --- | --- |
| `cargo fmt --all -- --check`、clippy 1.99（`--all-targets --locked -- -D warnings`、feature あり・なし） | 成功（最初の clippy が wasm の object writer の 2 件を指摘し、直した） |
| `RUST_MIN_STACK=4194304 cargo +1.99.0 test --locked --features fast-backend --no-fail-fast`（全体） | 成功: 112 target、1,622 件成功、0 失敗、1 件 ignored（lib 407、bin 32、`fast_backend` 7、`fast_backend_ops` 2） |
| `RUST_MIN_STACK=4194304 cargo +1.99.0 test --locked --no-fail-fast`（feature なし、全体） | 成功: 111 target、1,603 件成功、0 失敗、1 件 ignored（lib 397、bin 32）。feature ありとの差は feature つきの test の 19 件 |
| MSRV: `cargo +1.85.0 check --locked --features fast-backend` | **誤りだった**（2026-10-19 に訂正）。成功と報告したが、その実行の `rustc` は PATH の Homebrew の 1.98.1 で、1.85.0 は cargo だけだった。本物の rustc 1.85.0 では 196 件の `E0658`（let chain）で失敗する（N3） |
| Windows の型検査（`cargo check --all-targets --locked --features fast-backend --target x86_64-pc-windows-msvc`・`aarch64-pc-windows-msvc`） | 成功（2 target とも。stable 1.96.1。Windows での実行は未確認） |
| `node tests/fast_backend.mjs`（既定の 11 段）、`programs`（同じ出力 21・同じ export 85・差 0・比べないもの 12）、`debug`（21 project の 1,393 関数、行の欠けなし）、`wasm`（同じ挙動 96・差 0・比べないもの 22、module は 1.10 倍） | 成功。`programs`・`wasm` は `TSUZURI_STRINGS=compact` でも成功 |
| `node tests/fast_backend.mjs parity …`（`FAST_FORCE=1`、Node 24） | 成功: native 18 suite と wasm32 を含む 9 suite（Node 24）、`TSUZURI_STRINGS=compact` の 7 suite。fall back は `half`（7 件）と 1 行に複数の命令を書いた手書きの IR（1 件）だけで、suite の期待値は変えていない |
| Linux のコンテナ（AArch64 は native、x86-64 は Rosetta）: 実際の driver と、翻訳器の object だけの経路 | 成功（docs/benchmarks.md の測定 4。AArch64 は既定の mode・`programs`・`wasm`・cargo test、x86-64 は既定の mode・`programs`・cargo test を、統合後の木で） |
| `node tests/server.mjs`（`--backend fast` を含む）、`cargo test --test lto`、`node tests/pgo.mjs` | 成功（`server.mjs`: `--backend fast` の 3 つの command がサーバー経由と単独で同じ結果。`pgo.mjs`: 16 節成功。`lto` は全体の cargo test に含まれ 21 件成功） |
| `node scripts/check-docs.mjs`（変更した 4 ページ）、`git diff --check` | 成功（`debugging.md`・`option.md`・`platforms.md`・`usage.md`・`webassembly.md` の 5 ページ、178 リンク、11 例、native 10 回） |
| 旗なしの出力の不変: `src/llvm*.rs`・`src/runtime/` は統合 branch と差がない。統合 branch の `63a12d4a` を `git archive` から build した compiler と、この branch の compiler（feature あり・なし）で、`examples/`・`tests/fixtures/` の 118 project の `--emit llvm`（`-O0`・`-O3`）と、13 の example の `--emit object`・wasm32 の module（`-O0`・`-O3`）を作って比べた | 576 件すべてで、終了状態・stderr・成果物の byte が一致（差 0）。`tests/fast_backend.mjs` の既定 mode も `--backend llvm` を付けた IR と付けない IR を byte 比較する |

確認で見つけて直したこと（commit 済み）: clippy の 2 件（wasm の object writer）、feature なしの build で profile の option のエラーより先に「この `tsuzuri` に fast は入っていない」が出る（test の期待値を、feature の有無で決まるものにした）、`tests/server.mjs` の wasm32 の fast の command が成功を要求していた（エントリのない `42` の project では E2004 になるので、同じエラーを比べる）、`tests/fast_backend.mjs` が trap の signal を SIGTRAP と決めていた（x86-64 は SIGILL）、x86-64 の `i128` の署名の panic と `fmuladd`（WP14）。

### 計測

docs/benchmarks.md の PB05 の節（native と wasm32 のゲート、生成コードの速さ、大きさ、feature の費用、Linux・x86-64、生成コードの確認）。要点: ゲート（1,000 module の `build-edit-body`）は native 0.56 倍、wasm32 0.47 倍（200 module は 0.53 倍と 0.42 倍）。ビルドの残りは frontend が大半（81〜93%）で、翻訳器は native で total の 9〜12%、wasm32 で 6〜8%。生成コードの実行時間は `llvm -O0` の native 0.81 倍・wasm32 1.05 倍。成果物は native の実行ファイルが 15〜17%、wasm32 の module が 40〜43% 大きい。feature の費用（M5）は、`tsuzuri` が 9.47 MiB から 13.5 MiB（+43%）、依存が 9 から 46、`Cargo.lock` が 162 行から 473 行で、既定の build には入らない。測定は共有機の参考値（load average 10〜55）で、静かな機械での正式な比較は統合担当が行う。

### 見つけた問題・残作業

- Windows のホストで `--backend fast` を有効にするには、Windows で `windows_abi` の `__multi3` などの 128 bit の規約を確かめる必要がある（WP14）。有効にするときは `validate_fast` の `cfg!` を変え、`windows.yml` で `tests/fast_backend.mjs` を動かす。
- 配列と list の loop の wasm32 の速度（dispatch の loop を通る後ろ向きの分岐）。loop ごとに構造化した `loop` を置く変換をすれば、1.3〜1.5 倍の差は縮むはずだが、行っていない。
- Cranelift 0.122 以降は rust-version 1.86 以上が要る。MSRV を上げるときに、crate の版を上げて x86-64 の `enable_llvm_abi_extensions` などの設定を確かめる。
- 常駐サーバー（PB06）の `serve`・`watch` は `--backend` を要求の引数として運ぶ（`tests/server.mjs` が `--backend fast` を一度ずつ、サーバー経由と単独で比べる）。
- CI: `targets.yml` の 4 つの host に、feature つきの `cargo test --features fast-backend --test fast_backend --test fast_backend_ops` と `node tests/fast_backend.mjs`（既定・`programs`・`wasm`）を載せる。`windows.yml` は、有効にするまで型検査（`cargo check --all-targets --features fast-backend --target x86_64-pc-windows-msvc`）だけ。

### 更新したドキュメント

`README.md`（検証コマンド）、`docs/architecture.md`（図・module の表・翻訳器の節）、`docs/language.md`（`--backend`）、`docs/benchmarks.md`（PB05 の節）、`_tsuzuri/language-reference/compiler/option.md`（コード生成器・fast の WebAssembly・fast のデバッグ情報）、`usage.md`、`platforms.md`、`webassembly.md`、`debugging.md`。

### 変更したファイル

新規: `src/backend/`（`mod`・`module`・`parse`・`lex`・`func`・`types`・`abi`・`call`・`lower`・`ops`・`mem`・`intrinsics`・`data`・`debug`・`target`・`error`、`wasm/` の 8 ファイル）、`src/bin/tz-fast-cc.rs`、`tests/fast_backend.rs`・`fast_backend_ops.rs`・`fast_backend.mjs`、`benchmarks/run-backend.mjs`・`run-backend-kernels.mjs`・`control/kernels.c`。
変更: `Cargo.toml`・`Cargo.lock`（feature と crate）、`src/lib.rs`、`src/main.rs`（`--backend`）、`src/driver.rs`（`Backend`・`validate_fast`・build の flow）、`src/test_runner.rs`、`src/server.rs`（`sources()` が `src/bin/` を読み飛ばす）、`tests/server.mjs`。
