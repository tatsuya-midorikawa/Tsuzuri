# PR08: 言語間 LTO と bitcode 出力

| 項目 | 内容 |
| --- | --- |
| ID | PR08 |
| 分類 | 実行速度 |
| 優先度 | P2 |
| 規模 | M |
| 依存 | (PB01), (E12) |
| 関連 | E05, E06, E13, PR07 |
| 状態 | done |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`）。2026-10-17 に HEAD `9dd0fbbc` で再検討（「着手時の再検討」） |
| 承認 | 2026-10-17 の全フェーズ実装依頼で Phase 1・Phase 2（wasm32 の D10）を含む全フェーズと、D1〜D14（「着手時の再検討」で改訂した D1・D3〜D6・D8・D11・D13 と新規の D12・D14 を含む）を承認済み。`--emit bitcode` は新しい opt-in の出力で、既定の出力は変えない（測定で効果を確認できない既定の切り替えは行わない） |
| 手本にする既存実装 | C ランタイムの IR 化と結合: `src/driver.rs` の `build_complete` の `merge_debug_ir` 経路（`task.c` などを `-S -emit-llvm` で IR にし、`tool("TSUZURI_LLVM_LINK", "llvm-link")` で `module.ll` と結合する）。emit の追加: `Emit::Wgsl`（`src/driver.rs` の `Emit`・`BuildOptions::validate`・`BuildOptions::output_path`、`src/main.rs` の `--emit` の match）。ホストとの往復テスト: `tests/host_abi.rs` の `host_buffers_and_records_roundtrip_on_native_and_wasm`（IR の `@malloc`／`@free` を `@tracked_alloc`／`@tracked_free` へ置換して `live == 0` を確かめる） |
| 主な影響ファイル | `src/driver.rs`, `src/main.rs`, `src/cache.rs`（変更しないことの確認だけ）, `tests/lto.rs`（新規。ソースはテスト内に書き、fixture は作らない）, `docs/language.md`, `docs/architecture.md`, `docs/benchmarks.md`, `README.md`, `_docs/guides/native-interop.md`（改訂後の一覧は「着手時の再検討」の「作業の順序」。新規の `src/driver/lto.rs`・`benchmarks/run-lto.mjs`、`scripts/toolchain/clang.rs`、`tests/bindings.mjs` の期待値が加わる） |
| 計測対象 | ホストの C ループから小さな公開関数 `tz_add` を呼ぶ種目 `lto/small_call`（新規）。通常リンク（`--emit object`）・言語間 ThinLTO（`--emit bitcode`）・C だけの実装の 3 通りで、時間・ループ内の呼び出し命令の数・実行ファイルの大きさ |

## 目的

Tsuzuri の計算カーネルを C/C++/Rust のホストに組み込むとき、関数呼び出しの境界を越えてインライン展開・定数伝播ができるようにする。
細かい粒度でホストから呼ぶ場合の呼び出しコストをなくし、組み込み用途で C/C++ だけで書いた場合と同等の速度にする。

手段は新しい出力 `--emit bitcode`（新規）で、プログラムと必要なランタイムを一つの ThinLTO 用 LLVM bitcode として出す。
ホストは同じ LLVM の版の Clang（または linker plugin）で `-flto=thin` の LTO リンクを行う。
実装者は Phase 1（native）だけを実装する。Phase 2（wasm32 と `wasm-ld` の LTO）は人間が求めた場合だけ着手する。

## 着手時の再検討（2026-10-17）

2026-10-17 の依頼で、Phase 1（native の `--emit bitcode`）と Phase 2（wasm32 と `wasm-ld` の LTO）の両方を実装する。上の「目的」の最後の文（Phase 1 だけ）は取り下げる。
以下の「着手条件と停止条件」「現状と計測（HEAD `f8dc655`）」「設計」「実装手順」「決定事項」は起票時の記録で、このセクションと食い違う箇所はこのセクションが優先する
（旧 `_docs/` のパスは GUIDE §8.1 の表で `_tsuzuri/language-reference/` に読み替える。作業場所の `/tmp/tz-pr08` は `target/perf/PR08/work`）。
着手時の HEAD は `a2313d19`（main `9dd0fbbc` + 第4フェーズの docs の足場）。偵察は main `9dd0fbbc` の release コンパイラ（SHA-256 `5e1b9687…`）、Apple M1 Max、macOS 27.0.1、
Apple clang 21.0.0（clang-2100.3.34.2）、Homebrew の LLVM 21.1.8・22.1.8・23.1.1 と LLD 23.1.1、rustc 1.98.1（LLVM 22.1.8）、配布物 toolchain（darwin arm64。`clang`・`llvm-link` 21.1.8、Zig 0.16.0）で行った。
偵察のスクリプトと生データは `target/perf/PR08/`（Git に入れない）。第2・第3フェーズが入った後のコードと照合し、起票時の設計が通る部分と通らない部分を分けた。

### 起票時の記述と現行のコード

| 起票時 | 現行 | 影響 |
| --- | --- | --- |
| `Emit` は 6 つ。`--emit` は `exe`・`object`・`llvm`・`header`・`wasm`・`wgsl`、エラー文もその 6 語 | `Emit` は 14（`WgslRelaxed`・`Spirv`・`SpirvRelaxed`・`BindingsJs`・`Shared`・`BindingsCs`・`BindingsPy`・`BindingsCpp` が加わる）。文は `emit kind must be exe, object, llvm, header, wasm, wgsl, wgsl-relaxed, spirv, spirv-relaxed, shared, bindings-js, bindings-cs, bindings-py, or bindings-cpp` で、`src/main.rs` の parse と単体テスト、`tests/bindings.mjs` が 1 字違わず検査する | `bitcode` を `shared` の次に足す。変えてよいのは停止条件が認める「一覧だけ」。`tests/bindings.mjs` と単体テストの期待値を同じ変更で直す。`output_path` の網羅 match に `"bc"` |
| `BuildOptions::validate` で見るのは `--cpu native` の検査だけ | emit ごとの規則が `validate_shared`・`validate_allocator`・`validate_triple` に分かれている。`--allocator host`・`counting` は `Executable` 以外なら通り、`--freestanding` は `Object`・`Llvm`・`Header` だけ、`--trap-mode return` は `Object`・`Llvm`・`Header`・`Shared`・`Bindings*` だけ、`--strip` は `Executable` だけ、リンク入力は `Executable`・`Shared` だけ、bare metal は `--freestanding` の `Object`・`Llvm`・`Header` だけ | `validate_bitcode`（局所の新規 1 つ）を足し、既存の規則には `--trap-mode return` の許可リストへ `Bitcode` を足すだけ（文は変えない。`Shared` の前例）。`--freestanding`・bare metal・`--strip`・リンク入力は既存の文のまま bitcode を拒否する |
| `build_complete` の object 経路は `merge_debug_ir` と `-r -nostdlib` の結合。C は `task.c`・`cpu.c`・`io.c` | C ランタイムは `os.c`・`net.c`・`async.c`・GPU・`trap.c`・task・`cpu.c`・`io.c`・`arguments.c` を連結して `task.c` 1 つにし、`RuntimeSymbols` の接頭辞（`tsuzuri_cpu_`・`tsuzuri_io_` など）で選ぶ。`options.cache && !merge_debug_ir` のときは `compile_runtime_object("native-c")` の bundled／cache の object。`merge_debug_ir`（macOS・`-g`・C ランタイムあり・object）の 3 つのコマンドは実測で確認（`clang -std=c11 -c -fPIC -O3 -S -emit-llvm task.c`、`llvm-link module.ll task.o -S -o combined.ll`、`clang -x ir -Wno-override-module -O3 -g -fPIC -c combined.ll`） | 起票の「`merge_runtime_ir = merge_debug_ir \|\| Bitcode`」で足りる。C の IR 化は cache を通さない（bitcode では毎回 0.2 s 前後。D7） |
| `trap.c` の扱いの記述なし | `trap_runtime = trap_return && matches!(emit, Object \| Shared)` のときだけ連結する（「LLVM 出力は host が `trap.c` を持つ」） | `Bitcode` を足す。足さないと `--trap-mode return` の bitcode に `tsuzuri_boundary_run` の定義がない |
| 実行ファイルだけの剪定（PB03）は起票時にない | `emit_native_build` は `Entry::Console` だけ到達関数に絞る。`Entry::Library` の object・bitcode は全関数 | bitcode は object と同じ IR（剪定なし）。変更なし |
| probe の flags は `native_compile_args` と `native_cpu_flag` | 実行系の引数は `compile_args(platform)`（`--target TRIPLE` の `--target=`・`-fPIC`）と `native_ir_cpu_args(arch, f16)`（`-mavx512fp16`・`-mf16c` を含む。D10 の f16 の経路）、`no_sdk_args` | probe は `compile_args` と、`--cpu native` なら `native_ir_cpu_args`。f16 の旗を落とすと `"target-features"` が実際のコンパイルと食い違う |
| 外部定義は `tz_*`・`tsuzuri_alloc`・`tsuzuri_free` だけ。object の `nm -g` は 8 記号 | object の `nm -g` は 32 記号（`tsuzuri_cpu_*` の全種と `tsuzuri_trap_hooks`）。bitcode（C ランタイムを `--internalize` した後）の外部定義は `tz_*`、`tsuzuri_alloc`・`tsuzuri_free`（weak）、`tsuzuri_trap_hooks`（weak hidden）、`tz_soft_*`（numeric。weak hidden）、`--allocator small` の `tz_small_shared`・`tz_small_stats`（weak）に、条件つきで `tsuzuri_alloc_stats`・`tsuzuri_try_*`・`tsuzuri_async_*`・`tsuzuri_main`。`tsuzuri_cpu_*`・`tsuzuri_task_*` などの C の記号は、C ランタイムを持つ 13 構成すべてで internal | D5 と受け入れ条件・手順 6 の期待を差し替える（「ない」ものを検査する） |
| D3: `target-cpu`・`target-features`・`tune-cpu` の 3 鍵を、取り除いた group の末尾か新しい group で足す | `llvm-as` は inline の `"target-features"` より group の鍵を優先する（順序に関係なく）。F08 の `@cpu` 版は inline の `"target-features"="+avx2"` などを持つ `define` で（`tests/fixtures/cpu_versions` で x86-64 Linux 42 個、AArch64 Linux 28 個。`Platform::cpu_levels` が macOS では 0 なので macOS の IR には版がない）、起票の規則だと全部の版が基底の feature に戻る | D3 を改訂（下）。`emit_native_build_for` が levels を取るので、どのホストでも x86-64・AArch64 Linux の版の IR を単体テストで作れる |
| 3 鍵を写せば C のホストと互換になる | `target-features` を写すとホストの Clang との差で inline が外れる（Apple の probe を Homebrew の host へ: `+bti` などが多い。AArch64 Linux の Zig の host に Apple の probe: `-fmv` などが違う）。`target-cpu`（と `tune-cpu`）だけなら、Apple 21・Homebrew 21・22 の host で inline される。LLVM 23 は `tune-cpu=apple-m5` も要る | D3 改訂: 汎用は `target-cpu`・`tune-cpu` だけ、`--cpu native` だけ `target-features` も写す。「host は同じ Clang で作る」を規則にする（D14） |
| 再現: ThinLTO で `bl <_tz_add>` が残り（`grep -c` 2）、full LTO の remark が `conflicting attributes` | 同じ結果を再現（`4999999950000000`、2、同じ remark）。Homebrew の LLVM 23 の remark は `conflicting target features`。x86-64 Linux と AArch64 Linux（Zig 同梱 lld、ELF）は属性なしでも inline される | 属性の付与は Mach-O の AArch64 で効き、ELF では無害。効果を測るのは macOS で、Linux は CI の host テストで確認する |
| 「host が full LTO、Tsuzuri 側が ThinLTO は inline されない」 | Apple の `ld` は、付与後なら host・bitcode の ThinLTO と full LTO のどの組でも inline する。lld は host の full LTO + ThinLTO の bitcode が `linking module flags 'EnableSplitLTOUnit': IDs have conflicting values` で link エラー、host の ThinLTO + full LTO の bitcode は link できて inline されない | 文書は `-flto=thin` を必須にする（D2 のまま）。lld の誤用は黙らずエラーになる |
| `-fuse-ld=lld`（Homebrew）は `Unsupported stack probing method` | 原因は lld ではなく Apple clang の関数属性 `"probe-stack"="__chkstk_darwin"`（C ランタイムの IR に 10 か所、host の C にも）。Homebrew の LLVM 21.1.8・22・23 の LTO はどれも同じ abort。Apple clang で作った C ランタイム入りの bitcode は Apple の clang でしか読めない。C ランタイムを含まない bitcode には出ない | 組み合わせの表（下）。D6: 版の検査はしないが、文書に書く |
| `llvm-link` は Xcode になく `TSUZURI_LLVM_LINK=/opt/homebrew/opt/llvm@21/bin/llvm-link`（README の macOS debug object と同じ組） | README に `TSUZURI_LLVM_LINK` の説明はない（`compiler/option.md`・`compiler/usage.md` にある）。CI の `Rust tests` は `TSUZURI_CLANG=$LLVM_PREFIX/bin/clang` を設定するが `TSUZURI_LLVM_LINK` は設定せず、`llvm-link` は PATH にない（Homebrew は keg-only で確認。apt は版つきの名前の見込み）。`resolve_tool` は環境変数、同梱、PATH の順。`llvm-link` は macOS の配布物にだけ入り、Linux の配布物にはない。C ランタイムが要るプログラムは多い（`Array.sum` が `tsuzuri_cpu_sum_i64` を使う） | D4 改訂: bitcode だけ「環境変数、同梱、Clang の隣、PATH」の順で探す。`merge_debug_ir` の探し方は変えない |
| （起票時にない）配布物の `tsuzuri-clang` | `scripts/toolchain/clang.rs` は `-x ir` の compile へ `-O*`・`-W*`・`-m*`・`-g`・`-fPIC`・`-ffunction-sections`・`-fdata-sections` だけを渡し、`*.ll` の入力を要する（PB03 の記録と同じ）。`-flto=thin` は黙って落ち、`file` は `Mach-O 64-bit object` を示す（bitcode ではない成果物を `.bc` と呼んでしまう）。C は `zig cc`（`-S -emit-llvm` は通り、IR の triple は `arm64-apple-macosx12.0.0-unknown`）。同梱の `clang`・`llvm-link` と組んだ模擬は同じ外部記号で bitcode になった | D12（新規）: launcher が `-flto*` を渡す変更と、成果物の magic の検査。PR07 も同じ filter を触る |
| PB01 の事前ビルド object は bitcode に使わない | 同じ。`runtime_objects` は `(Native, Executable)`・`(Wasm32, Wasm)` だけで、bitcode は `numeric.ll`・`math.ll` を IR に含める。`numeric.ll`・`math.ll` の属性 group は起票時と同じ（`"tune-cpu"="generic"`・`"no-builtins"`・`"min-legal-vector-width"`・`"no-trapping-math"`。`target-cpu` はない） | 変更なし |
| `define` はすべて 1 行で `{` で終わる、後置は `#N`・`!dbg` だけ | `examples/*`・`tests/fixtures/*` のうち `--emit llvm` を出せる 102 のディレクトリ（`--trap-mode return` は 96）× 通常・`-g`・`--trap-info`・`--trap-mode return`・`--allocator small` の IR と全ランタイム `.ll` で確認: 1 行・`{` 終わり、後置は inline の属性（`nounwind`・`alwaysinline`・`noinline`・`cold`・`memory(...)`・`allockind(...)`・`"alloc-family"=...`）・`unnamed_addr`・`#N`・`!dbg`・`unnamed_addr  {`（二重の空白）・属性なし。`section`・`align`・`comdat`・`personality` はない | 挿入規則（`!dbg` の前、なければ ` {` の前）は成り立つ。停止条件は「inline の `"target-features"`」と「上記以外の後置」に置き換える |
| `ld -r` で結合した object は他言語 LTO に参加しない | 同じ。ただし `--emit bitcode` は Windows の COFF object で未対応の「C ランタイム入り object」（E2002）を迂回する経路でもある | Windows は D13 |
| `lto/small_call` は PX01 が todo なら生データだけ | PX01 は done（`benchmarks/metrics.mjs`、`benchmarks/run-*.mjs`、`docs/benchmarks.md` の `phase4:PR08` の marker） | PX01 形式の JSONL（suite `lto`）と runner `benchmarks/run-lto.mjs` |

### 偵察で測った事実（2026-10-17）

- **属性なしの再現**: `tz_add` は ThinLTO のリンクで inline されず（`bl` が `main` の中に残る）、full LTO の remark は `conflicting attributes`。`stamp`（起票の D3）で全 `define` に `target-cpu`・`target-features` を付けると inline され、ループは定数 `4999999950000000` に畳まれる（Apple clang 21 の host、host と同じ probe）。
- **どの stamp がどの host で inline されるか**（macOS arm64、`tz_add` の呼び出しが `main` に残る数、0 が inline。bitcode の作成と host のコンパイル・リンクは同じ列の Clang）:

| Tsuzuri 側に付けた属性 | Apple 21.0.0 | Homebrew 21.1.8 | Homebrew 22.1.8 | Homebrew 23.1.1 |
| --- | --- | --- | --- | --- |
| なし | 1 | 1 | 1 | 1 |
| Apple の probe の `target-cpu`・`target-features` | 0 | 1 | 1 | 1 |
| Apple の probe の `target-cpu` だけ | 0 | 0 | 0 | 1 |
| Homebrew 21 の probe の `target-cpu`・`target-features`（または `target-cpu` だけ） | 0 | 0 | 0 | 1 |
| Homebrew 23 の probe の `target-cpu`・`tune-cpu`（`target-features` あり・なし） | 1 | 0 | 1 | 0 |
| `target-cpu="generic"`（±基本の feature） | 1 | 1 | 1 | 1 |

  読み: host の Clang と probe の Clang が同じ（`target-cpu`・`tune-cpu` が一致）なら inline される。features の全列を写すより `target-cpu` だけのほうが広い host で inline される。LLVM 23 だけ `tune-cpu` の一致も要る。別の CPU（`generic`）では tuning の bit が違うので inline されない。
- **ELF（Zig 0.16.0 の `zig cc -flto=thin`、lld 21）**: x86-64 Linux・AArch64 Linux の `tz_add` は、属性なしでも `target-cpu` だけでも inline された（x86-64 では `main` が定数 `0x11c37934e58f80`（= 4999999950000000。起票の `0x11c379e5348f80` は誤記）を `printf` に渡すだけになった。実行はしていない）。Apple clang が `--target=aarch64-unknown-linux-gnu` で出した `target-features`（`+fp-armv8,+neon,+v8a,-fmv`）を AArch64 Linux の bitcode に写すと inline されない。host の full LTO + ThinLTO の bitcode は上のとおりエラー、ThinLTO の host + full の bitcode は inline されず、full + full は inline される。リンク時に `warning: overriding the module target triple with x86_64-unknown-linux5.10.0-gnu2.31.0 [-Woverride-module]`（Zig の triple の綴りの違い。害はない）。
- **版の組（macOS、producer → consumer）**: `add`（C ランタイムなし）は、producer が Apple 21・Homebrew 21・22・23 の 16 の組のすべてで link でき（新しい producer の bitcode を古い consumer が読める場合がある）、inline は、producer が Apple・Homebrew 21・22 のとき consumer が Apple・Homebrew 21・22 で、producer が Homebrew 23 のとき consumer が Homebrew 21・23 で起きた（producer は自分の probe の `target-cpu`・`tune-cpu` を付けた）。C ランタイムを含む module は、LLVM 22・23 の bitcode を Apple 21・Homebrew 21 の LTO が `error: Unknown attribute kind (105) (Producer: 'LLVM23.1.1' Reader: 'LLVM APPLE_1_2100.3.34.2_0')` で拒否する。producer の識別は IDENTIFICATION に `APPLE_1_2100.3.34.2_0`（Apple）、`LLVM21.1.8`（Homebrew）。`llvm-link` が Clang より新しいと `-S` の出力を Apple clang が `error: unterminated attribute group`（`nocreateundeforpoison`）で読めず、binary 出力でも `Unknown attribute kind (105)`。Apple clang で作った C ランタイムの IR は `"probe-stack"="__chkstk_darwin"` を持ち、Homebrew の LLVM 21.1.8・22・23 の LTO が `LLVM ERROR: Unsupported stack probing method` で abort する。
- **決定性**: 同じ IR を別の path に置いて Clang で bitcode にした結果は `cmp` で一致する（`SOURCE_FILENAME` は `tsuzuri`）。bitcode の strtab に triple（OS の版つき。`macosx27.0.0`）が入る点は object と同じ。
- **C ランタイム結合の模擬**（`llvm-link --internalize`、14 構成。`ffi_extensions` を除く 13 が C ランタイムを持つ: `fix`（`add`・`sum`・`bytes`）の `-O0`・`-Oz`・`--trap-info`・`--allocator small`・`counting`・`--cpu native`、tasks・`--trap-mode return`・host_abi・ffi_extensions・cpu_kernels・async_reactor・io）は、すべて ThinLTO の bitcode になり、空の host と link できる。外部定義は上の表のとおり。`llvm.global_ctors`（`tz_sync_install`）が tasks・async・`--trap-mode return` に入る。`fix` の bitcode は 10,336 bytes（object 29,280 bytes）、C の host の実行ファイルは 50,560 bytes（object のリンクは 69,408 bytes）。冷えた状態の所要時間（M1 Max、共有機の参考値）は probe 0.03 s、C の IR 化 0.18〜0.20 s、`llvm-link` 0.05 s、bitcode 化 0.04〜0.14 s。`-g` も通る（host の DWARF 5 との `Dwarf Version` の警告が link で出る）。
- **host の言語**: C++（Apple の `clang++`）は C と同じく inline された。Rust は macOS で `-C linker-plugin-lto` が Apple の `ld` と `ld64.lld` のどちらでも `-plugin-opt=O3` を拒否されて link できない。`-plugin-opt` を落とす wrapper と Homebrew の clang 22.1.8（rustc と同じ LLVM）なら link でき結果は正しいが、`tz_add` は inline されない（remark に `tz_add` は出ない。Rust の関数は `"target-cpu"="apple-m1"`・`"probe-stack"="inline-asm"`）。rustc の LLVM（22）は CI の LLVM 21 より新しいので、LTO する linker の LLVM が rustc 以上でないと Rust の bitcode を読めない。Linux の Rust は std がなく未確認。
- **wasm32（Phase 2）**: LLVM の clang 21.1.8 で IR を `--target=wasm32-unknown-unknown -mbulk-memory -mno-simd128 -x ir -flto=thin -c` にし、host を `clang --target=wasm32-unknown-unknown -O3 -flto=thin -nostdlib -fuse-ld=lld -Wl,--no-entry -Wl,--export=tz_add` で `wasm-ld` の LTO にすると、import は空、export は `memory`・`run`・`tz_add`、`run(1000n)` は `499500n`、`tz_add` は inline された（ループは閉じた式になり `wasm.ll` の weak な `__multi3` を 1 回呼ぶ）。属性を付けても付けなくても同じ。Apple clang には wasm の backend がなく、LLVM の clang が要る（今の wasm ビルドと同じ条件）。（訂正: 実装で確かめると、Apple clang 21.0.0 の `clang --print-targets` に `wasm32`・`wasm64` があり、wasm32 の bitcode も作れた。「実装と検証」を参照）
- **配布物**: `tsuzuri-clang -x ir -flto=thin -c` は Mach-O の object を作り、同梱の `clang`（21.1.8）に直接渡すと bitcode を作る。`tsuzuri-clang -S -emit-llvm`（C）→ 同梱の `llvm-link --internalize` → 同梱の `clang -flto=thin` は、LLVM の Clang の場合と同じ外部記号の bitcode になった。

### 方針の見直し

| 決定 | 起票 | 改訂 | 理由と根拠 |
| --- | --- | --- | --- |
| D1 | `--emit bitcode`、`bc`、native だけ | 名前・拡張子は維持。native（Phase 1）に wasm32（Phase 2）を加え、wasm64 は E2000。Windows の target は E2000（D13） | 上の wasm32 の確認。Phase 1 の WP は起票の文（`'--emit bitcode' requires '--target native'; wasm32 bitcode is not supported yet`）を使い、WP6 で wasm32 を許すときに wasm64 向けの文へ改める |
| D2 | bitcode は `-flto=thin` | 維持 | lld は host の full LTO + ThinLTO の bitcode を、inline しないのではなく link エラーにする。full LTO の bitcode を選ぶ option は対象外のまま |
| D3 | 3 鍵を全 `define` に付ける | **改訂**: probe の `target-cpu` と `tune-cpu`（ある場合）だけを、自分の `"target-features"` を持たない `define` に付ける。`--cpu native` のときだけ probe の `target-features` も付ける（machine の ISA を object と同じに保つ）。自分の `"target-features"` を持つ `define`（F08 の `@cpu` 版）には `"target-cpu"`・`"tune-cpu"` だけの group を付け、`target-features` は変えない。既存の group は 3 鍵を取り除いてから同じ規則で足す。probe の flags は `compile_args` と、`--cpu native` なら `native_ir_cpu_args` | 起票の規則は F08 の版を黙って壊す（42・28 個の feature が消える）。`target-features` を写すと host の Clang との違いで inline が外れ、`target-cpu` だけのほうが広く inline される（上の表）。`alwaysinline`・inline の閾値の変更は引き続き使わない |
| D4 | C ランタイムは `-S -emit-llvm` → `llvm-link --internalize` | 維持。加えて `trap.c` を含める、`llvm-link` は「環境変数、同梱、Clang の隣、PATH」の順で探す（bitcode だけ）、`llvm-link` 実行後の Clang の失敗には llvm-link の版の hint を付ける、runtime の IR 化は cache しない | CI は `TSUZURI_LLVM_LINK` を設定せず llvm-link は PATH にないが、Clang の隣にはある（`$LLVM_PREFIX/bin`）。llvm-link が Clang より新しいと text IR が読めない（上の事実） |
| D5 | 外部定義は `tz_*`・`tsuzuri_alloc`・`tsuzuri_free` だけ | **改訂**: 公開の集合は `--emit shared` の export（`shared_exports`）と同じ（`tz_*`、`tsuzuri_alloc`・`tsuzuri_free`、条件つきで `tsuzuri_alloc_stats`・`tsuzuri_try_*`・`tsuzuri_async_*`・`tsuzuri_main`）。object にも出る weak の runtime 記号（`tz_soft_*`・`tsuzuri_trap_hooks`・`tz_small_*`）は許す。検査は「C ランタイムの記号（`tsuzuri_cpu_*`・`tsuzuri_task_*`・`tsuzuri_io_*`・`tsuzuri_os_*`・`tsuzuri_net_*`・`tsuzuri_gpu_*` ほか）が外部にない」。**訂正**: 公開の記号のうち C ランタイムだけが定義する `tsuzuri_async_post` は、外部に残す（「実装と検証」の「レビューの指摘への修正」） | 実測した外部定義（上の表）。object が今出す記号以外は増やさない |
| D6 | 版を検査しない | 維持。さらに成果物の先頭が bitcode の magic（`BC C0 DE` または Darwin の `DE C0 17 0B`）でなければ E2002（D12）。Apple clang の版数は LLVM の版と一致するとは限らないので `llvm-link --version` との突き合わせはしない。文書に producer の確認方法、実測した診断の文、組み合わせの表を書く | 版の不一致は host の LTO が `Unknown attribute kind` などで知らせる。launcher が黙って object を作る誤りは検査できる |
| D7 | bitcode は cache しない | 維持（上の所要時間は 0.3〜0.45 s）。cache は、実測で冷えた時間が利用者の障害になると分かった場合だけ別の WP で（key に `llvm-link` の identity を足すだけで足りる） | `build_key` は emit の違いと Clang の identity を既に含む。今回は効果が測れない最適化を入れない |
| D8 | `-O<n>`・`--cpu native`・`-g`・`--trap-info` は object と同じ意味 | 維持。加えて `--trap-mode return`・`--allocator system`・`small`・`host`・`counting` と `-Os`・`-Oz` も object と同じ意味で通す。`--freestanding`・bare metal・Windows・`--strip`・リンク入力は拒否 | `--freestanding` と bare metal は既存の文のまま拒否する（C ランタイムも llvm-link も使わない形で、対応するなら別の変更） |
| D9 | llvm-link のないとき C ランタイムのテストは skip | 維持。ただし Clang の隣から探すので CI（macOS・Linux の LLVM 21）では実行される。skip の行は明示する | 偵察で macOS の Homebrew の layout（`clang`・`llvm-link`・`llvm-nm` が同じ `bin`）を確認。Linux の apt（`/usr/lib/llvm-21/bin`）は同じ形の見込みで、CI で確かめる |
| D10 | Phase 2 は人間が求めた場合だけ | **承認済み**。上の確認で実現できる | wasm32 では C ランタイムも `llvm-link` も要らない。`--wasm-feature threads`・`jspi`・`webgpu`・`net`、`--wasm-host wasi`、wasm64 は Phase 2 でも対象外（E2000）。`simd128` は通す（probe の flags に `-msimd128`、feature の付与は Phase 2 で小さい matrix を測って決める） |
| D11 | 新しい module は作らない | **改訂**: 子 module `src/driver/lto.rs`（`pub mod lto;`）に新規の関数を置き、`driver.rs` は呼び出しの数行だけにする | `driver.rs` は PR07・PB05・PB06 も同時に直す。子 module は親の private な `tool`・`run_tool`・`compile_args`・`driver_error` をそのまま使え、テストは `tsuzuri::driver::lto::...` で使える |
| D12 | （なし） | **新規**: 配布物の launcher が `-x ir` の compile へ `-flto`・`-flto=thin` を渡すようにし（`scripts/toolchain/clang.rs`）、driver は Clang の後に成果物の magic を検査して、bitcode でなければ E2002 にする | 偵察の launcher の挙動。launcher を直しても、別の wrapper や古い launcher が同じ誤りを起こしうる |
| D13 | Windows は未検証（拒否はしない） | **改訂**: `platform.windows()`（MSVC・MinGW）の target は E2000（`'--emit bitcode' is not supported on Windows yet (G10); build an object with --emit object and link it normally`）。Windows の CI で確かめてから外す | `--emit shared` の前例。Windows の挙動は緑の Windows CI でしか確認できず、`windows_abi` の補助関数・COFF の LTO の規則（`lld-link`）をここでは試せない |
| D14 | （なし） | **新規**: 文書の規則は「host は `TSUZURI_CLANG` と同じ Clang（同じ提供元・同じ LLVM の版）でコンパイルしリンクする。リンクは `-flto=thin`。linux は `-fuse-ld=lld`、タスクは `-pthread`、数学は `-lm`」。inline は保証せず、効果は host と probe の Clang の一致で決まることを表で示す | 上の表と版の組の実測 |

### 作業の順序（コミット）

| 作業 | 内容 | 確認 |
| --- | --- | --- |
| WP0 | この再検討の記録（このチケットだけ） | `git diff --check` |
| WP1 | `Emit::Bitcode`・`output_path`・`validate_bitcode`・CLI（`--emit` の一覧と文、`--help`）・単体テスト、`tests/bindings.mjs` の文 | `cargo test --locked --bin tsuzuri`・`--lib`、`node tests/bindings.mjs`（N を記録） |
| WP2 | `src/driver/lto.rs`: `stamp_target_attributes`（D3 改訂）・`probe_target_attributes`・magic の検査・`llvm-link` の探索。まだ配線しない。単体テストと、`emit_native_build_for` の x86-64・AArch64 Linux の levels を含む全 fixture の IR を通す検査 | `cargo test --locked --test lto`（N≥1） |
| WP3 | `build_complete` の配線（C ランタイムなし、あり、`trap.c`、cache の除外、hint）と launcher の `-flto*`、`merge_debug_ir` の既存の挙動と既定の出力が不変であること | 手作業の模擬と同じ外部記号、`benchmarks/check-handoff-equivalence.mjs` と `shasum` で既定の出力が baseline と同じ、`node tests/handoff_tools.mjs`、配布物の smoke（`node scripts/toolchain/smoke.mjs <bundle>`）に bitcode の magic の検査を足す |
| WP4 | host テスト（C・C++・tracked allocator の `live == 0`・runtime 結合・magic の拒否。fake の `TSUZURI_CLANG` は CLI の子 process で）。単体テストの件数を記録 | `cargo test --locked --test lto`、`TSUZURI_LLVM_LINK` なしでも通る |
| WP5 | 計測 `benchmarks/run-lto.mjs`（suite `lto`、workload `small_call`、variant `object`・`bitcode`・`c_only`、9 回交互）と `docs/benchmarks.md` の `phase4:PR08` の節 | 3 通りが同じ出力、M1・M2・M3 |
| WP6 | Phase 2: wasm32（validate、probe の flags、`wasm-ld`、import が空であることの node での検査、テスト。wasm64 の文） | Node 24、`cargo test --locked --test lto` |
| WP7 | 文書（`compiler/native-interop.md`・`option.md`・`usage.md`、`docs/language.md`・`architecture.md`、`README.md`） | `node scripts/check-docs.mjs <変更したページ>` |
| WP8 | 全 Rust テスト・関係する E2E・Windows の型検査（`cargo +stable check --all-targets --target x86_64-pc-windows-msvc`）・`## 実装と検証`・`_completed/` へ移動 | §5 の検証一覧 |

### 受け入れ条件の変更

- 「外部に定義される記号が `tz_*`・`tsuzuri_alloc`・`tsuzuri_free` だけ」→ D5 の集合（C ランタイムの記号が外部にない、公開の集合は `shared_exports` と同じ）。object の `nm -g` の期待（8 記号）は 32 記号に更新。
- 「`tz_add` がホストのループに inline され、呼び出し命令 0」→ host と bitcode を同じ Clang で作る構成（macOS の Apple clang と、CI の LLVM 21）で 0。別の Clang の組は inline を保証せず、表に実測を書く。
- 「`cargo test --locked --test lto` が `5 passed`」→ WP4 の件数（stamp の単体、全 fixture の IR、probe、inline、tracked allocator、runtime 結合、C++、拒否の検査、magic の拒否、Phase 2 の wasm32）。`llvm-link`・`llvm-nm`・wasm の backend のない環境の skip は明示の行つきで成功する。
- 「`--target wasm32` は E2000」→ Phase 2 の後は wasm64・Windows が E2000 で、wasm32 は通る。
- 追加: 配布物の launcher と、bitcode でない成果物の拒否（D12）。既定の出力（`--emit llvm`・`object`・`exe`・`header`・`shared`・`wasm`）は baseline と byte 単位で同じ（`benchmarks/check-handoff-equivalence.mjs`）。
- 「未検証」の表に、Rust・Linux の実行・Windows・wasm64 を実測つきで書く（Rust の失敗は停止ではなく表に記録する）。

### テスト計画の変更

- 共通（`tests/lto.rs`。`tests/host_abi.rs` と同じく pid と時刻の一時ディレクトリ、`TSUZURI_CLANG` か `clang`）: host と bitcode は同じ Clang で作り、host のリンクは `-O3 -flto=thin`。Linux は `-fuse-ld=lld`（`ld.lld` が見つからなければ skip の行）、非 Windows は `-lm`、タスクを含む構成は `-pthread`。`llvm-link`・`llvm-nm` は環境変数、Clang の隣の順に探し、なければ skip の行を出して成功する。Windows は `cfg!(windows)` で E2000 の検査だけして成功する。期待値は C の意味と手計算（`n(n-1)/2`）から決める。`unsafe` を使わないので、環境変数を変える検査（fake の `TSUZURI_CLANG`）は CLI（`env!("CARGO_BIN_EXE_tsuzuri")`）の子 process で行う。
- 起票の 5 テストに、次を足す・変える: (1) stamp の単体（inline の `"target-features"` を持つ `define` が変わらないこと、`!dbg`・`#N` の再利用・`unnamed_addr  {`・`define` のない入力、2 回で同じ結果、`--cpu native` の差）。(2) 全 fixture と `examples/*` の IR（x86-64・AArch64 Linux の levels を `emit_native_build_for` で作ったものを含む）を stamp して Clang が読めること。(3) probe（`#[cfg(unix)]`）。(4) C の host が `tz_add` を inline し（`-O0`・`-O3`、`nm` に `tz_add` がない、stdout `4999999950000000`）、2 回の bitcode が byte 一致し（`cache: false`）、先頭が magic。(5) tracked allocator の `live == 0`。(6) C ランタイムの結合（`sum`）で、外部に C の記号がないこと（`llvm-nm`）と `tz_sum` の結果。(7) C++ の host（`--driver-mode=g++`）。(8) 拒否の組み合わせ（Windows、`--link`、`--strip`、`--freestanding`、wasm64。Phase 1 の間は wasm32）と、`--trap-mode return`・`--allocator small` が通ること。(9) fake の Clang が object を書くと E2002（unix）。(10) Phase 2: wasm32 の bitcode と `wasm-ld` の LTO で、import が空・`tz_add` の結果・inline（LLVM の Clang と `wasm-ld` と Node 24 が要り、なければ skip の行）。
- Rust の host は CI に入れない（macOS は `-plugin-opt` を拒否され、LLVM 21 の linker は rustc の LLVM 22 の bitcode を読めない）。手順と失敗の出力は文書の表に記録する。新しい `tests/*.mjs` は作らない。`src/main.rs` の単体テストと `tests/bindings.mjs` の `emit kind must be ...` の文は WP1 で更新する。

### 停止条件の更新

- 取り下げ: 「`#<n>` と `!dbg` 以外の後置要素を持つ `define`」（inline の属性は通る。`section`・`align` などは 0 個）、「`tz_add` が inline されない（`conflicting attributes` 以外の理由）」（host と probe の Clang が違うと inline されないことが分かっているので、同じ Clang の構成で見る）。
- 追加: 自分の `"target-features"` を持つ `define` の feature が、stamp の後で変わった。`llvm-link --internalize` の後に C ランタイムの記号が外部に残った。既定の出力（object・exe・llvm・header・shared・wasm）の byte が変わった。launcher を直した後も `-flto=thin` の成果物が bitcode でない。

### 未検証・非対応の見込み（実装後に確定して `docs/benchmarks.md` と native-interop に書く）

| 組み合わせ | 偵察の結果 | 扱い |
| --- | --- | --- |
| macOS、host と Tsuzuri が同じ Apple clang 21 または Homebrew の LLVM 21（`-flto=thin`、Apple の `ld`） | 確認済み（C・C++ で inline） | テストで固定 |
| macOS、host と bitcode の Clang が違う（Apple と Homebrew、LLVM 22・23） | link はするが inline は保証しない（表） | 文書に表。Apple で作った C ランタイム入りの bitcode は Homebrew の LTO で abort |
| macOS の `-fuse-ld=lld` | Apple clang の `probe-stack` の属性で abort（Homebrew の LTO 全般） | 非対応と理由を書く |
| Linux x86-64・AArch64（lld） | Zig 0.16.0 で ELF を cross-link し inline を確認（実行は未確認） | Linux の CI の host テストで確認する。確認前は「リンクと inline を確認、実行は未確認」 |
| Rust の `-C linker-plugin-lto` | macOS は `-plugin-opt` を拒否され link できない。wrapper なら link できるが inline されない。LLVM 21 の linker では rustc（LLVM 22）の bitcode を読めない | 手順を未検証と書き、失敗の出力を記録。CI には入れない |
| Windows（MSVC・MinGW） | 試せない | E2000（D13）。Windows の CI で確かめてから外す |
| wasm32 + `wasm-ld`（Phase 2） | 手作業で確認（import 空、inline） | Phase 2 でテストに固定。wasm64・threads・jspi・webgpu・net・WASI は対象外 |

## 着手条件と停止条件

### 着手条件

- 依存の PB01・E12 は括弧付き（任意）で、開始条件にしない。PR08 はランタイムの C を今と同じくビルドのたびに Clang で
  コンパイルする。PB01 が先に done になっていても、その object キャッシュは bitcode に使えない（object は LTO に参加しない）ので流用しない。
  状態の確認: `grep -n "PB01\|PR08" _perfs/README.md` と `grep -n "E12" _features/README.md`。
- GUIDE §2.3 の基準コマンドが成功し、実装手順 1 のベースライン（IR・object の記号・手作業の LTO 実験）を保存していること。
- 道具: `clang --version` の 1 行目が `Apple clang version 21.0.0 (clang-2100.3.34.2)`（2026-09-30 に確認）、
  `/opt/homebrew/opt/llvm@21/bin/` に `llvm-link`・`llvm-objdump`・`llvm-bcanalyzer` があること。Apple の Xcode には `llvm-link` がないので、
  ランタイム C を含む bitcode は `TSUZURI_LLVM_LINK=/opt/homebrew/opt/llvm@21/bin/llvm-link` で作る（README の macOS debug object と同じ組）。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- 属性の付与（D3）で、1 行で終わらない `define`（行末が `{` でない）や、`#<n>` と `!dbg` 以外の後置要素を持つ `define` が見つかった。
- `--emit llvm`・`--emit object`・`--emit exe` の生成 IR が 1 byte でも変わった（bitcode 以外の経路は変えない。手順 1 の保存物と比べる）。
- `llvm-link --internalize` の後で `tz_*`・`tsuzuri_alloc`・`tsuzuri_free` が internal になった、またはランタイム C の記号が外部に残った。
- 属性を付けても手順 7 の検査で `tz_add` がホストのループに inline されない（remark に `conflicting attributes` 以外の理由が出る場合を含む）。
- 既存テストの期待値（IR・診断メッセージ・`--emit` の一覧以外の CLI 出力）を変える必要がある。`emit kind must be ...` の一覧だけは変えてよい。
- `unsafe`、新しい crate、LLVM の C API、既定の WASM import が必要になった。
- Rust との組み合わせが失敗した。これは停止ではなく、失敗の出力を文書の「未検証・非対応」の表に記録して先へ進む。

## 現状と計測（HEAD `f8dc655`）

### 出力とシンボル

- `src/driver.rs` の `Emit` は `Executable`・`Object`・`Llvm`・`Header`・`Wasm`・`Wgsl` の 6 つ。`src/main.rs` の `--emit` は
  `exe`・`object`・`llvm`・`header`・`wasm`・`wgsl` を受け、それ以外は `error[E2000]: emit kind must be exe, object, llvm, header, wasm, or wgsl`（終了コード 2）。
  値の名前は小文字の 1 語で、拡張子は `BuildOptions::output_path` が決める（`o`・`ll`・`h`・`wasm`・`wgsl`）。
- `--emit llvm` の IR（`src/llvm.rs` の `emit_target`）は `source_filename = "tsuzuri"` だけで、`target triple`・`target datalayout` を持たない。
  関数の属性は `nounwind` などの直書きだけで、`"target-cpu"`・`"target-features"` を持たない。
- 公開 wrapper は `src/llvm_abi.rs` の `wrapper` が `.replacen("define internal ", "define ", 1)` で作る `define i64 @tz_add(...)`（外部・既定の可視性）。
  内部関数は `define internal`。拡張 ABI の `tsuzuri_alloc`・`tsuzuri_free` は `define weak`（同ファイルの定数文字列）。
- 埋め込みランタイムは `src/llvm.rs` が必要に応じて IR へ連結する `.ll`（`string.ll`・`utf8string.ll`・`heap-native.ll`・`closure.ll`・`numeric.ll`・
  `math.ll` など）。`numeric.ll`・`math.ll` は `x86_64-unknown-linux-gnu` 向けに `generate.py`・`generate_math.py` で生成し、属性グループに
  `"target-cpu"` を持たない（`numeric.ll` の `attributes #0` は `"tune-cpu"="generic"` を持つ）。
- C のランタイム `task.c`・`cpu.c`・`io.c` は IR に含まれない。`build_complete` は IR の `declare void @tsuzuri_task_parallel(`・
  `declare i64 @tsuzuri_cpu_sum_i64(`・`declare i32 @tsuzuri_io_` を検索し（`task_runtime`・`cpu_runtime`・`io_runtime`）、exe・object のときだけ Clang で
  コンパイルして結合する。記号は `TZ_TASK_API`・`TZ_CPU_API`・`TZ_IO_API`（`__attribute__((weak, visibility("hidden")))`）。object は
  `clang -r -nostdlib`（macOS は `-Wl,-keep_private_externs`）で一つにする。
- ビルドキャッシュ（`crate::cache::build_key`）は `Emit::Llvm | Emit::Header | Emit::Wgsl` 以外で Clang を key に含める。

### 再現（2026-09-30 に確認）

`/tmp/tz-work-PR08/lib/Main.tz` を次の 2 行とし、C のホストから 1 億回呼ぶ。

```tsuzuri
export def add :: i64 -> i64 -> i64
fn add x y = x + y
```

```c
#include <stdint.h>
#include <stdio.h>

int64_t tz_add(int64_t x, int64_t y);

int main(void) {
    int64_t total = 0;
    for (int64_t i = 0; i < 100000000; i++) total = tz_add(total, i);
    printf("%lld\n", (long long)total);
    return 0;
}
```

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
W=/tmp/tz-work-PR08
target/release/tsuzuri build $W/lib --emit llvm -O3 -o $W/lib.ll
clang -O3 -flto=thin -x ir -Wno-override-module -c $W/lib.ll -o $W/lib.o
clang -O3 -flto=thin -c $W/host.c -o $W/host.o
clang -O3 -flto=thin $W/host.o $W/lib.o -o $W/host-lto && $W/host-lto
/opt/homebrew/opt/llvm@21/bin/llvm-objdump -d --no-show-raw-insn $W/host-lto | grep -c tz_add
```

| 実験 | 結果 |
| --- | --- |
| 上のとおり（属性なし、ThinLTO） | `4999999950000000`。`bl <_tz_add>` がループに残る（`grep -c` が 2） |
| 同じ IR で full LTO（`-flto`）、リンクに `-Wl,-mllvm,-pass-remarks-missed=inline` | inline されない。remark: `'tz_add' not inlined into 'main' because it should never be inlined (cost=never): conflicting attributes` |
| IR の全 `define` にホストの C と同じ `"target-cpu"="apple-m1"`・`"target-features"="+aes,..."` を付けて full LTO | inline され、ループ全体が定数 `0x11c379e5348f80`（= 4999999950000000）に畳まれる。`grep -c` が 0 |
| 同じく属性付きで ThinLTO | inline される（`grep -c` が 0） |
| ホストが full LTO（`-flto`）、Tsuzuri 側が ThinLTO | inline されない（`grep -c` が 2）。ThinLTO の module と通常 LTO の module の間では import しない |
| リンク行に `.bc` の名前で渡す | Clang が `.bc` を入力として一度コンパイルし直す（`-###` で `"-cc1"` が 1 回）が、inline はされる |
| `-fuse-ld=lld`（Homebrew の lld） | `LLVM ERROR: Unsupported stack probing method` で異常終了。macOS では Apple の `ld` を使う |
| 生成 bitcode の producer | `llvm-bcanalyzer -dump` の IDENTIFICATION が `APPLE_1_2100.3.34.2_0`。ThinLTO 版は `GLOBALVAL_SUMMARY_BLOCK` を 1 つ持ち、full LTO 版は持たない |

原因: IR 入力の関数に `"target-cpu"` がないと、LTO の target machine の既定 CPU（Apple の `ld` では C の `apple-m1` と異なる）で扱われ、
AArch64 の inline 互換検査（callee の機能ビットが caller の部分集合）に失敗する。Clang は IR 入力の関数へ属性を補わない（上の bitcode を
`llvm-dis` すると `tz_add` の属性は `mustprogress nofree norecurse nosync nounwind willreturn memory(none)` だけ）。

### 既存の計測

- `docs/benchmarks.md` の「測定条件」「C++20 とのネイティブ比較」などは、ネイティブの公開関数を外部 ABI で呼び、LTO を使わない。
  LTO の有無による時間はまだ測っていない。
- [なぜ Tsuzuri か](https://github.com/tatsuya-midorikawa/Tsuzuri/blob/c82c13e1e3dd1f02f78694aa1d26d39b3f793504/_docs/learn/why-tsuzuri.md) は、ホスト境界には引数変換・呼び出しなどのコストがあり、細かすぎる呼び出しは不利になり得ると説明している。

## 目標と指標

目標は CI の合否条件にしない。専用の計測機で before と after を記録して判断する。

- G1: `--emit bitcode` の出力と C のホストを ThinLTO でリンクすると、ホストのループから `tz_add` の呼び出し命令が消える。
- G2: 種目 `lto/small_call` の時間が、同じ関数を C で書いた場合と同等（差が広がりの範囲内）。
- G3: `--emit bitcode` 以外の出力（exe・object・llvm・header・wasm・wgsl）の生成 IR と結果を変えない。
- G4: 同じ入力・同じ Clang からの bitcode は byte 単位で一致する。

| 指標 | 単位・統計 | 対象 | 期待（計測で確かめる） |
| --- | --- | --- | --- |
| M1 呼び出し命令 | 個（固定値） | `lto/small_call` の実行ファイルの `_main` 内の `tz_add` への `bl`／`call` | object 1 → bitcode 0 |
| M2 時間 | ms（9 回の中央値、最小、最大） | `lto/small_call`、規模 `n = 1000000000` | bitcode と C だけの実装の差が広がりの範囲内。object はそれより遅い |
| M3 実行ファイルの大きさ | bytes（固定値） | M2 の 3 通りの実行ファイル | 記録する（未使用のランタイムが LTO で消えるので bitcode ≤ object を見込む） |

## 変えてはいけない意味

- LTO は呼び出しの境界を消すだけで、Tsuzuri 側の検査（借用バッファの長さ・null・alignment、UTF-8 の検証、`tsuzuri_alloc` の負の size の
  トラップ）を省略しない。検査は `tz_*` wrapper の本体にあり、inline 後も残る。
- 整数の折り返し（`add i64` に `nsw`/`nuw` を付けない）、浮動小数点の丸め・NaN・符号付きゼロ・fast-math なし、評価順序、トラップ
  （`llvm.trap`）を保つ。属性の付与（D3）は `"target-cpu"`・`"target-features"`・`"tune-cpu"` の 3 つだけを写す。`"unsafe-fp-math"`・
  `"no-nans-fp-math"`・`"no-trapping-math"`・`"frame-pointer"` などは写さない。
- 所有権の契約（結果バッファはホストが `tsuzuri_free` で一度だけ解放）と確保の釣り合い（`live == 0`）を変えない。
- 公開する記号は通常の object と同じく `tz_*`・`tsuzuri_alloc`・`tsuzuri_free` だけ。内部関数・ランタイムの名前は internal か hidden のまま。
  `tsuzuri_` 接頭辞の規則（docs/language.md「公開 ABI」）を保ち、新しい外部記号を足さない。
- 決定性: 同じ入力・同じ Clang・同じ `--cpu` なら bitcode は byte 単位で一致する（G4）。
- WASM の既定の import なし（D-18）は Phase 1 では触らない。

## 設計

### 出力の形

- `--emit bitcode`（新規）は `--target native` だけで使え、拡張子は `bc`。中身は ThinLTO の要約（`GLOBALVAL_SUMMARY_BLOCK`）を持つ一つの
  bitcode で、生成 IR（埋め込みの `.ll` ランタイムを含む）と、必要なときだけ C のランタイム（`task.c`・`cpu.c`・`io.c`）の IR を含む。
- IR は object と同じ `llvm::emit_native_build` で作る。このため `--trap-info`（`<output>.trap.json`）・`--debug-info`（`-g`）・`--cpu native` は
  object と同じ意味で使える。エントリーは `Entry::Library`（`main` を出さない）。
- `-O<n>` は bitcode を作る前の最適化（ThinLTO の pre-link）の段を決める。リンク時の最適化の段はホストのリンク行の `-O<n>` が決める。
- ビルドキャッシュは使わない（D7）。

### LLVM の版

- bitcode を書くのは設定された Clang（`TSUZURI_CLANG`、なければ `clang`）。版は bitcode の IDENTIFICATION に自動で入る
  （このマシンでは `APPLE_1_2100.3.34.2_0`）。確認は `clang --version` と `llvm-bcanalyzer -dump x.bc | grep -m1 "record string"`。
- 規則: LTO を行うリンカー（linker plugin）の LLVM は、すべての bitcode の producer と同じか新しくなければならない。コンパイラは版を検査しない（D6）。
  不一致はホストのリンクで LLVM の bitcode reader のエラーとして出る（文言は版ごとに違う。`Unknown attribute kind`・`Invalid record` や
  `Producer: '...' Reader: '...'` を含むのが典型。未検証）。
- 組み合わせの確認状況（2026-09-30）:

| ホスト側 | Tsuzuri 側 | リンク | 状態 |
| --- | --- | --- | --- |
| Apple clang 21 の C、`-flto=thin` | Apple clang 21、属性付き（手作業） | `clang -flto=thin`（Apple の `ld`） | 確認済み（inline される） |
| Apple clang 21 の C、`-flto`（full） | Apple clang 21、ThinLTO | 同上 | 結合はできるが inline されない（D2） |
| Apple clang 21 の C | Apple clang 21 | `-fuse-ld=lld`（Homebrew） | 異常終了（非対応） |
| rustc 1.98.1（LLVM 22.1.8）の `--emit=llvm-bc`、C のホスト | なし | Apple の `ld` | 1 例で結合・inline された（保証しない） |
| Rust の `-C linker-plugin-lto` | `--emit bitcode` | Apple の `ld` または `lld` | 未検証 |
| Linux の Clang の C | `--emit bitcode` | `clang -flto=thin -fuse-ld=lld` | 未検証（手順 9 のテストが Linux の CI で通れば確認済みに変える） |
| Windows | `--emit bitcode` | `lld-link` | 未検証（拒否はしない） |

### データ構造

```rust
pub enum Emit {
    Executable,
    Object,
    Llvm,
    Header,
    Wasm,
    Wgsl,
    Bitcode, // 新規
}

/// Returns `"target-cpu"=... "target-features"=... ["tune-cpu"=...]` that `clang` gives a C function with `flags`.
pub fn probe_target_attributes(clang: &OsStr, flags: &[&str], directory: &Path) -> Result<String, Diagnostic> // 新規
/// Gives every `define` in `ir` the target attributes, replacing those in the attribute groups the definitions use.
pub fn stamp_target_attributes(ir: &str, attributes: &str) -> String // 新規
```

2 関数は `src/driver.rs` に置く（新しい module は作らない）。`tests/lto.rs` から使うので `pub`。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| CLI | `src/main.rs` | `--emit` の match、使い方の `--emit KIND` の行 | `Some("bitcode") => Emit::Bitcode`。エラーは `emit kind must be exe, object, llvm, header, wasm, wgsl, or bitcode`、使い方は `exe, object, llvm, header, wasm, wgsl, or bitcode` |
| driver | `src/driver.rs` | `Emit` | `Bitcode` |
| driver | `src/driver.rs` | `BuildOptions::validate` | `emit == Emit::Bitcode && target != Target::Native` を E2000（診断の表）。`--cpu native` の検査は変えない（`Emit::Llvm`・`Emit::Header` だけを拒否するので bitcode は通る） |
| driver | `src/driver.rs` | `BuildOptions::output_path` | `Emit::Bitcode => "bc"`（網羅 match なので arm がないと compile error） |
| driver | `src/driver.rs` | `build_complete` の `matches!(options.emit, Emit::Executable \| Emit::Object)`（`llvm::emit_native_build` を選ぶ条件） | `Emit::Bitcode` を加える |
| driver | `src/driver.rs` | `build_complete` の `let cache = if options.cache && options.emit != Emit::Header` | `&& options.emit != Emit::Bitcode` を加える（D7） |
| driver | `src/driver.rs` | `build_complete` の object 経路（`module.ll` を書く `else` 節） | `module.ll` を書く前に bitcode なら probe と属性の付与。ランタイム C のコンパイルに `-S -emit-llvm` を足す条件を `merge_debug_ir \|\| bitcode` に広げ、`llvm-link` の結合も同じ条件にする。bitcode のときだけ `--internalize` を足し、hint を bitcode 用にする。最後の `clang` に bitcode のときだけ `-flto=thin` を足す（`-c` と出力先 `artifact` は既存の条件で決まる） |
| driver | `src/driver.rs` | `probe_target_attributes`・`stamp_target_attributes` | 新規（アルゴリズム） |
| driver | `src/driver.rs` | そのほかの `Emit::` の比較（`-lm`、exe へのランタイム object、relocatable link、`wasm-ld`、`run_with_diagnostics`、Windows COFF の拒否） | 変更なし（どれも `Executable`・`Object`・`Wasm` だけを見るので bitcode は正しく外れる） |
| cache | `src/cache.rs` | `build_key` | 変更なし（bitcode はキャッシュの前に外れる） |

### 生成 IR とランタイム

```text
text = llvm::emit_native_build(...).ir            // object と同じ IR（Windows は llvm::windows_abi も同じ）
attributes = probe_target_attributes(clang, [native_compile_args..., native_cpu_flag?], temporary)
text = stamp_target_attributes(text, attributes)
write temporary/module.ll
if native_runtime:
    clang -std=c11 -c <native_compile_args> -O<n> [-pthread] [-g] -S -emit-llvm [-mcpu=native|-march=native] task.c -o task.o
    llvm-link module.ll task.o --internalize -S -o combined.ll      // TSUZURI_LLVM_LINK
clang -x ir -Wno-override-module -O<n> [-g] <native_compile_args> [cpu] -c -flto=thin <ir> -o artifact
```

| 記号 | bitcode での linkage | 備考 |
| --- | --- | --- |
| `tz_*` | 外部・既定の可視性 | ホストが呼ぶ。実行ファイルへの LTO では inline 後に消えてよい |
| `tsuzuri_alloc`, `tsuzuri_free` | `weak` | 拡張 ABI を使うときだけ（現状のまま） |
| `tz.*`、埋め込み `.ll` の関数 | `internal` | 現状のまま |
| `tz_soft_op`（`numeric.ll`） | `weak hidden` | 現状のまま |
| `tsuzuri_task_*`, `tsuzuri_cpu_*`, `tsuzuri_io_*` | `internal`（`--internalize`） | object では hidden。bitcode ではリンク後に外へ出ない |
| `malloc`, `free`, `write`, pthread など | `declare` | ホストのリンクで解決する。task runtime を含むときホストは `-pthread` を付ける |

`llvm-link --internalize` は最初のファイル（`module.ll`）以外から結合した記号だけを internal にする（`module.ll` の `tz_*` は外部のまま）。
`module.ll` は `target triple` を持たないので、結合で C 側の triple と datalayout が入る（既存の `merge_debug_ir` 経路と同じ。警告は無視してよい）。

### アルゴリズム

`probe_target_attributes`:

1. `directory/probe.c` に `void tsuzuri_probe(void) {}` を書き、`clang -std=c11 -S -emit-llvm <flags> probe.c -o probe.ll` を `run_tool` で実行する
   （hint は `bitcode output requires Clang that emits LLVM IR; set TSUZURI_CLANG`）。
2. `define` で始まり `@tsuzuri_probe(` を含む行から `#<n>` を取り、`attributes #<n> = {` の行を探す。
3. その行から `"target-cpu"="…"`・`"target-features"="…"`・`"tune-cpu"="…"` をこの順で取り出し、空白 1 つで連結して返す。
   `"target-cpu"` がなければ E2002 `cannot read target attributes from Clang output; set TSUZURI_CLANG to a Clang that emits LLVM IR`。

`stamp_target_attributes`（行単位。正規表現の crate は使わない）:

1. `attributes #<n> = {` の行の最大の `n` を求め、`k = n + 1`（なければ 0）とする。
2. `define ` で始まる各行: 行末は `{` でなければならない（違えば停止条件）。` #<数字>` を含むなら、その番号を集合に入れる。含まないなら
   ` !dbg ` の直前、なければ行末の ` {` の直前に ` #k` を挿入する。
3. 集合に入った番号の `attributes #<n> = { ... }` の行から 3 つの鍵（`"key"="` から次の `"` まで）とその前の空白を取り除き、`}` の直前に
   `attributes` を足す。
4. 末尾に `attributes #k = { <attributes> }` を 1 行足す（ステップ 2 で挿入した行がなければ足さない）。

`declare` の行と呼び出し位置の属性グループは変えない。入力が同じなら出力も同じ（決定的）。

### Phase 分割

- Phase 1（このチケット）: native の `--emit bitcode`、C ホストとの ThinLTO の手順・テスト・計測、Rust の手順（未検証と明記）。
- Phase 2（設計方針。人間が求めた場合だけ）: `--target wasm32 --emit bitcode`。IR に `wasm.ll` を含め、probe は
  `--target=wasm32-unknown-unknown -mbulk-memory` と `-msimd128`／`-mno-simd128` で行う。ホストは
  `clang --target=wasm32-unknown-unknown -flto=thin -nostdlib -Wl,--no-entry -Wl,--export=tz_<name>` で `wasm-ld` の LTO を行い、
  `--emit wasm` と同じ export（`memory`、拡張 ABI の `tsuzuri_alloc`・`tsuzuri_free`）を出す。import が空であることを node で確かめる。
  `--wasm-feature threads`（`task-wasm-threads.c`）は Phase 2 でも対象外。

### 診断

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| E2000 | `--emit bitcode` と `--target wasm32` | `'--emit bitcode' requires '--target native'; wasm32 bitcode is not supported yet` | `<command line>` |
| E2000 | 不明な emit の種類 | `emit kind must be exe, object, llvm, header, wasm, wgsl, or bitcode`（既存の文の一覧を更新） | `<command line>` |
| E2002 | probe の出力に `"target-cpu"` がない | `cannot read target attributes from Clang output; set TSUZURI_CLANG to a Clang that emits LLVM IR` | `<command line>` |
| E2002 | `llvm-link` を実行できない・失敗した | 既存の `run_tool` の形（`cannot execute '<name>': <error>; <hint>` または `'<name>' failed (<status>):` と出力と hint）。hint は `bitcode with the task, CPU, or IO runtime requires llvm-link matching Clang; set TSUZURI_LLVM_LINK` | `<command line>` |
| E2002 | probe・bitcode 化の Clang の失敗 | 同じ `run_tool` の形。hint は probe が上の文、bitcode 化は既存の `install LLVM/Clang 17+ or set TSUZURI_CLANG to its executable` | `<command line>` |

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N を必ず見る（GUIDE §3.1）。作業用の場所は `/tmp/tz-pr08/`。

### 手順 1: ベースライン

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行する。`/tmp/tz-pr08/fix/Main.tz` を次の内容で作り（2026-09-30 に `--emit llvm`・`--emit object` で確認済み）、
  他の出力の hash を保存する。「再現」の手作業の実験も一度行い、`grep -c tz_add` が 2 であることを見る。
- 確認: 次がすべて成功し、`before.sha` が 4 行になる。`cargo test --locked --test host_abi` の `running N tests` の N を記録する。

```tsuzuri
export def add :: i64 -> i64 -> i64
fn add x y = x + y
export def sum :: ref [i64] -> i64
fn sum values = Array.sum values
export def bytes :: i64 -> [ubyte]
fn bytes count = new [ubyte](count, index -> index as ubyte)
```

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
T=target/release/tsuzuri; W=/tmp/tz-pr08
$T build $W/fix --emit llvm -O3 -o $W/fix.ll
$T build $W/fix --emit header -o $W/fix.h
for o in 0 3; do $T build $W/fix --emit object -O$o -o $W/fix-O$o.o; done
shasum -a 256 $W/fix.ll $W/fix.h $W/fix-O0.o $W/fix-O3.o > $W/before.sha && wc -l $W/before.sha
nm -g $W/fix-O3.o | grep -v ' U '
cargo test --locked --test host_abi
```

`nm -g` は `_tsuzuri_alloc`・`_tsuzuri_cpu_features`・
`_tsuzuri_cpu_sum_i64`・`_tsuzuri_cpu_variant`・`_tsuzuri_free`・`_tz_add`・`_tz_bytes`・`_tz_sum` を出す（2026-09-30 に確認）。

### 手順 2: 属性の付与（純粋関数）

- 変更: `src/driver.rs` の `stamp_target_attributes`（新規）、`tests/lto.rs`（新規）。
- 内容: 「アルゴリズム」のとおりに実装する。テスト `stamps_every_definition_and_existing_groups` を足す（テスト計画）。まだどこからも呼ばない。
- 確認: `cargo test --locked --test lto` が `1 passed`。

### 手順 3: 属性の probe

- 変更: `src/driver.rs` の `probe_target_attributes`（新規）、`tests/lto.rs`。
- 内容: 「アルゴリズム」のとおり。`run_tool` と `driver_error` を使う。テスト `probes_clang_target_attributes` を足す。
- 確認: `cargo test --locked --test lto` が `2 passed`。

### 手順 4: `Emit::Bitcode` と CLI

- 変更: `src/driver.rs` の `Emit`・`BuildOptions::validate`・`BuildOptions::output_path`、`src/main.rs` の `--emit` の match と使い方、
  `src/main.rs` の tests。
- 内容: 「段ごとの変更」の CLI と driver の行。この手順では `build_complete` をまだ変えないので、`build` は `Emit::Bitcode` を object と同じ経路で
  処理してしまう。手順 5 まで CLI から試さない。`src/main.rs` の tests に `parse(&["build", "Main.tz", "--emit", "bitcode"])` が `Emit::Bitcode` になる
  assert と、`--target wasm32` との組み合わせが `validate` で E2000 になる assert を足す（既存の `--emit wgsl` の assert の隣）。
- 確認: `cargo test --locked --bin tsuzuri` が成功し、N が 1 以上増える。`cargo test --locked --lib validates_native_cpu_tuning_and_selects_architecture_flags` が `1 passed`。

### 手順 5: `build_complete` の配線

- 変更: `src/driver.rs` の `build_complete`。
- 内容: 「段ごとの変更」の `build_complete` の 3 行。probe の flags はランタイム C のコンパイルと同じ `native_compile_args(cfg!(windows), env::consts::ARCH)` と、
  `options.cpu == Cpu::Native` のときの `native_cpu_flag(env::consts::ARCH)?`。`merge_debug_ir` の既存の意味（macOS・debug・object）は変えず、
  `let merge_runtime_ir = merge_debug_ir || options.emit == Emit::Bitcode;`（新規の局所変数）で分岐を広げる。
- 確認: 次が成功する。`file` は `LLVM bitcode`（macOS は `LLVM bitcode, wrapper`）を表示し、`cmp` は何も出さない。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri && cargo build --release --locked
T=target/release/tsuzuri; W=/tmp/tz-pr08; B=/opt/homebrew/opt/llvm@21/bin
mkdir -p $W/lib && printf 'export def add :: i64 -> i64 -> i64\nfn add x y = x + y\n' > $W/lib/Main.tz
$T build $W/lib --emit bitcode -o $W/add.bc && file $W/add.bc
$T build $W/lib --emit bitcode -o $W/add2.bc && cmp $W/add.bc $W/add2.bc
TSUZURI_LLVM_LINK=$B/llvm-link $T build $W/fix --emit bitcode -o $W/fix.bc && file $W/fix.bc
$T build $W/lib --emit bitcode --target wasm32 -o $W/x.bc; echo "exit=$?"
```

最後の行は `error[E2000]: '--emit bitcode' requires '--target native'; wasm32 bitcode is not supported yet` と `exit=2`。

### 手順 6: 生成物の検査

- 変更: なし。
- 内容: 「生成コードの確認」の bitcode の項（要約・producer・属性・記号）を `add.bc` と `fix.bc` に行う。
- 確認: すべての期待どおり。とくに `fix.bc` の外部定義が `tz_add`・`tz_bytes`・`tz_sum`・`tsuzuri_alloc`・`tsuzuri_free` だけで、
  `tsuzuri_cpu_` で始まる記号が外部に出ないこと。

### 手順 7: C ホストとの LTO と inline

- 変更: なし。
- 内容: 「再現」のホスト `host.c` を `$W/host.c` に置き、`--emit bitcode` の出力で LTO リンクする。
- 確認: 出力が `4999999950000000`、`grep -c` が 0（macOS。ELF では、残った定義のラベルの 1 行を数える。「Linux の CI の指摘への修正」）。

```sh
W=/tmp/tz-pr08; B=/opt/homebrew/opt/llvm@21/bin
clang -O3 -flto=thin -c $W/host.c -o $W/host.o
clang -O3 -flto=thin $W/host.o $W/add.bc -o $W/host-lto && $W/host-lto
$B/llvm-objdump -d --no-show-raw-insn $W/host-lto | grep -c tz_add
```

### 手順 8: テスト

- 変更: `tests/lto.rs`。
- 内容: テスト計画の残り 3 テストを足す。
- 確認: `TSUZURI_LLVM_LINK=/opt/homebrew/opt/llvm@21/bin/llvm-link cargo test --locked --test lto` が `5 passed`。`TSUZURI_LLVM_LINK` なしでも
  `5 passed`（runtime のテストは skip の行を出して成功する）。

### 手順 9: 既存の出力が変わらないこと

- 変更: なし。
- 内容: 手順 1 のコマンドを再実行して hash を比べる。全体のテストを流す。
- 確認: `shasum -a 256 -c /tmp/tz-pr08/before.sha` がすべて `OK`。`cargo test --locked` が成功する。
  `cargo test --locked --test host_abi` が手順 1 と同じ N で成功する。

### 手順 10: 計測

- 変更: なし。
- 内容: 「計測手順」のとおり。
- 確認: `target/perf/PR08-after/` に 3 通りの時間・大きさの記録があり、3 通りとも出力が `499351563280`。

### 手順 11: 文書

- 変更: 「ドキュメント」の各ファイル。
- 内容: 手順・版の規則・確認状況の表・計測結果を書く。
- 確認: `node scripts/check-docs.mjs _docs/guides/native-interop.md` が成功する。`git diff --check` が何も出さない。

## 計測手順

### 環境

専用の計測機で、ほかの重い処理を止めて行う。次を記録の先頭に残す。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
P=target/perf/PR08-after; mkdir -p $P
{ git rev-parse HEAD; git status --short | wc -l; sw_vers; sysctl -n machdep.cpu.brand_string; clang --version | head -1;
  rustc -vV | grep LLVM; shasum -a 256 target/release/tsuzuri; } > $P/env.txt
```

### 種目 `lto/small_call`

`$P/small_call.c` を次の内容で作る。`values` は実行時に作り、`n` は引数なので、ループ全体の定数化は起きない。`c_add` は Tsuzuri の
`i64` の `+`（`add i64`、折り返し）と同じ意味を未定義動作なしで書いたもの。

```c
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

int64_t tz_add(int64_t x, int64_t y);
static int64_t c_add(int64_t x, int64_t y) { return (int64_t)((uint64_t)x + (uint64_t)y); }
#ifndef ADD
#define ADD tz_add
#endif

int main(int argc, char **argv) {
    int64_t n = argc > 1 ? atoll(argv[1]) : 1000;
    int64_t values[1024];
    for (int i = 0; i < 1024; i++) values[i] = (int64_t)(i * 2654435761u % 1000u);
    int64_t total = 0;
    for (int64_t i = 0; i < n; i++) total = ADD(total, values[i & 1023]);
    printf("%lld\n", (long long)total);
    return 0;
}
```

独立の参照（Python、2026-09-30 に計算）: `n = 1000` で `498932`、`n = 1000000000` で `499351563280`。

```sh
P=target/perf/PR08-after; T=target/release/tsuzuri
mkdir -p $P/lib && printf 'export def add :: i64 -> i64 -> i64\nfn add x y = x + y\n' > $P/lib/Main.tz
$T build $P/lib --emit object -O3 -o $P/add.o
$T build $P/lib --emit bitcode -O3 -o $P/add.bc
clang -O3 -c $P/small_call.c -o $P/host.o && clang -O3 $P/host.o $P/add.o -o $P/object
clang -O3 -flto=thin -c $P/small_call.c -o $P/host-lto.o && clang -O3 -flto=thin $P/host-lto.o $P/add.bc -o $P/bitcode
clang -O3 -flto=thin -DADD=c_add $P/small_call.c -o $P/c_only
for v in object bitcode c_only; do echo "$v $($P/$v 1000) $($P/$v 1000000000)"; done
```

3 行とも `498932 499351563280` でなければ計測しない（正しさが先）。

### 時間（M2）

ウォームアップ 1 回の後、3 通りを交互に 9 回ずつ実行する。

```sh
P=target/perf/PR08-after
for v in object bitcode c_only; do $P/$v 1000000000 > /dev/null; done
for r in 1 2 3 4 5 6 7 8 9; do for v in object bitcode c_only; do
  /usr/bin/time -p $P/$v 1000000000 2>> $P/$v.time > /dev/null; done; done
for v in object bitcode c_only; do echo "$v $(awk '/^real/ {print $2 * 1000}' $P/$v.time | sort -n | tr '\n' ' ')"; done
```

各行の 5 番目が中央値、1 番目と 9 番目が最小と最大（ms）。

### 大きさ（M3）と呼び出し命令（M1）

```sh
P=target/perf/PR08-after; B=/opt/homebrew/opt/llvm@21/bin
for v in object bitcode c_only; do echo "$v $(stat -f %z $P/$v) $($B/llvm-objdump -d --no-show-raw-insn $P/$v | sed -n '/<_main>:/,/^$/p' | grep -c tz_add)"; done
```

Linux では `stat -c %s` を使い、`<_main>` を `<main>` に読み替える。

### 記録

- 生データ（`*.time`・`env.txt`・上の出力）は `target/perf/PR08-after/` に残す（Git には入れない）。
- PX01 が done（`benchmarks/metrics.mjs` がある）なら、同じ値を PX01 形式（suite `lto`、workload `small_call`、variant `object`・`bitcode`・`c_only`、
  `samples` は 9 個の ms）で `target/perf/PR08-after/lto.jsonl` に書く。PX01 が todo なら生データだけでよい。
- 中央値・最小・最大・大きさ・M1 と環境を `docs/benchmarks.md` に表で書く。差が広がりの範囲内なら「差なし」と書き、速いと主張しない。

## 生成コードの確認

### bitcode

```sh
W=/tmp/tz-pr08; B=/opt/homebrew/opt/llvm@21/bin
$B/llvm-bcanalyzer -dump $W/add.bc | grep -c '<GLOBALVAL_SUMMARY_BLOCK'
$B/llvm-bcanalyzer -dump $W/add.bc | grep -m1 'record string'
$B/llvm-dis $W/add.bc -o - | grep -E '^define|^attributes'
$B/llvm-nm --defined-only --extern-only $W/fix.bc
```

- 1 行目は `1`（ThinLTO の要約がある）。2 行目は `record string = 'APPLE_1_2100.3.34.2_0'`（このマシン）。
- 3 行目: すべての `define` が属性グループ `#<n>` を持ち、それらのグループがすべて `"target-cpu"="apple-m1"` と `"target-features"="+aes,` で
  始まる機能の列を含む（`--cpu native` なら probe の CPU 名）。`"unsafe-fp-math"`・`"no-nans-fp-math"` が現れない。
- 4 行目: 名前が `tsuzuri_alloc`・`tsuzuri_free`・`tz_add`・`tz_bytes`・`tz_sum` だけ。

### 機械語（native `-O3`）

- 手順 7 と M1: bitcode の実行ファイルの `_main` に `tz_add` への `bl` がない。object の実行ファイルにはある。
- inline の理由を見るときは、リンク行に `-Wl,-mllvm,-pass-remarks=inline -Wl,-mllvm,-pass-remarks-missed=inline`（macOS の `ld`）を足す。
  `'tz_add' inlined into 'main'` を含む行が出て、`conflicting attributes` が出ないこと。Linux の `lld` では `-Wl,-plugin-opt=-pass-remarks=inline`（未検証）。

### ほかの出力

- 手順 9 の `shasum -a 256 -c` で、llvm・header・object（`-O0`・`-O3`）が HEAD と byte 単位で同じであること。

## テスト計画

### Rust テスト（`tests/lto.rs`、新規）

補助関数: 一時ディレクトリ（`tests/host_abi.rs` と同じく pid と時刻の名前）、`TSUZURI_CLANG` か `clang`、ソース文字列を自分専用の一時 root の
`Main.tz` に書いて `tsuzuri::driver::Project::load` する `project(source)`（E03 の再帰探索のため、テストごとに root を分ける）。
期待値は Tsuzuri の出力ではなく手計算・C の意味から決める。

| テスト | 内容と期待 |
| --- | --- |
| `stamps_every_definition_and_existing_groups` | 入力は `declare void @llvm.trap()`、`define internal i64 @f(i64 %x) nounwind {`、`define i64 @tz_f(i64 %x) nounwind !dbg !7 {`、`define internal void @g(ptr %p) local_unnamed_addr #0 {`（本体つき）と `attributes #0 = { nounwind "no-trapping-math"="true" "tune-cpu"="generic" }`。属性 `"target-cpu"="cpu-x" "target-features"="+a,+b"` で、`@f` の行は `nounwind #1 {`、`@tz_f` の行は `nounwind #1 !dbg !7 {`、`@g` の行と `declare` は不変、`#0` は `{ nounwind "no-trapping-math"="true" "target-cpu"="cpu-x" "target-features"="+a,+b" }`、末尾に `attributes #1 = { "target-cpu"="cpu-x" "target-features"="+a,+b" }`。期待する全文を文字列で書いて `assert_eq!`。2 回呼んで同じ結果。`define` のない入力は不変 |
| `probes_clang_target_attributes` | `#[cfg(unix)]`。`probe_target_attributes(&clang, &["-fPIC"], &root)` が `"target-cpu"="` で始まり ` "target-features"="` を含む。2 回の結果が同じ |
| `bitcode_inlines_small_exports_into_c_host` | `add` だけのソース（「再現」）。`-O0`・`-O3` で `Emit::Bitcode` を `tsuzuri::driver::build` し、2 回の出力が byte 一致、先頭 4 byte が `BC\xC0\xDE` か `\xDE\xC0\x17\x0B`（Darwin の wrapper）。「再現」の `host.c` と `clang -O3 -flto=thin host.c <out.bc> -o <exe>` でリンクして実行し、stdout が `4999999950000000\n`（`n(n-1)/2`、`n = 10^8`）。`nm <exe>` の出力に `tz_add` がない（macOS だけで成り立つ。ELF は残すので、実装は、実行ファイルに `tz_add` を指す命令がないことで見る。「Linux の CI の指摘への修正」）。`--target wasm32` の `BuildOptions::validate` が E2000 |
| `bitcode_keeps_owned_results_balanced` | `add` と `bytes`（手順 1 の 2 関数）。`tsuzuri::llvm::header` で `api.h`、`tsuzuri::llvm::emit_target(&module, Entry::Library, false)` の IR の `@malloc`／`@free` を `@tracked_alloc`／`@tracked_free` に置換し（`tests/host_abi.rs` と同じ）、`probe_target_attributes` と `stamp_target_attributes` を通して `clang -x ir -Wno-override-module -O3 -flto=thin -c` で `module.o` にする。ホストは `tests/host_abi.rs` の `tracked_alloc`・`tracked_free` を写し、`tz_bytes(&out, 5)` の中身が `0 1 2 3 4`、`tsuzuri_free(out.ptr)` と `tsuzuri_free(tsuzuri_alloc(0))` の後に `live == 0`、`tz_add(40, 2) == 42` を assert する。リンクは `-O0` と `-O3`（ともに `-flto=thin`）。同じホストを置換済みの `.ll` と LTO なしでリンクした結果と stdout が一致する。バッファ型の名前は生成 header のもの（`tsuzuri_ubyte_buffer` の想定。header で確かめる） |
| `bitcode_merges_c_runtime_and_internalizes_it` | `sum`（`Array.sum`。native では `tsuzuri_cpu_sum_i64` を使う）。`TSUZURI_LLVM_LINK` がなければ `skipped: set TSUZURI_LLVM_LINK to llvm-link matching Clang` を出して成功する（D9）。あれば bitcode を作り、`int64_t v[] = {1, 2, 3}` の `tz_sum(v, 3)` が `6`。`llvm-link` と同じディレクトリに `llvm-nm` があれば、`llvm-nm --defined-only --extern-only` の名前が `tz_sum`・`tsuzuri_alloc`・`tsuzuri_free` の部分集合 |

`src/main.rs` の tests: `--emit bitcode` の parse と、`emit kind must be ...` を assert している箇所があれば一覧の更新（手順 4）。

### E2E

- 他の出力の生成 IR を変えないので、既存の E2E suite は変わらない。新しい `tests/*.mjs` は作らない（C のホストが要るので Rust テストで行う）。
- native の `-O0`・`-O3` は上の 3・4 番目のテストで確かめる。WASM は Phase 1 で触らない（`tests/host_abi.rs` の WASM 部分が不変のまま通る）。
- トラップ: `tests/host_abi.rs` が既存の経路で確かめる。bitcode でも同じ wrapper なので新しいトラップのテストは足さない。

### 既存テストへの影響

`emit kind must be ...` の文の一覧だけ（あれば）。それ以外はなし。

### 性能

「計測手順」のとおり。合否の閾値は作らない。

## ドキュメント

- `docs/language.md`「公開 ABI」: 末尾の `sh` の例に `tsuzuri build Kernel.tz --emit bitcode -o kernel.bc` を足し、ThinLTO（ホストは `-flto=thin`）・
  LLVM の版の規則・公開される記号（D5）を 3〜5 行で書く。
- `_docs/guides/native-interop.md`: 節「言語間 LTO（bitcode）」（新規）。C のリンク手順（手順 7 のコマンド）、Rust の手順（下の例。未検証と明記）、
  版の確認方法、「LLVM の版」の組み合わせの表、`-fuse-ld=lld`（macOS）の非対応。
- `README.md`「CLI」の emit の一覧と、「ビルド」の `TSUZURI_LLVM_LINK` の説明（bitcode でランタイム C を含むときにも要る）。
- `docs/architecture.md`: driver の段の説明に bitcode の経路（probe・属性の付与・`llvm-link --internalize`・`-flto=thin`）を 2〜4 行。
- `docs/benchmarks.md`: 節「言語間 LTO（PR08）」（新規）に環境・3 通りの中央値・最小・最大・大きさ・M1。「測定条件」の「LTO は行いません」は
  他の比較には引き続き当てはまることを残す。
- `_perfs/README.md` の PR08 の状態（GUIDE §10 の手順で更新）。

Rust の例（新 API（実装後に有効。未検証））。`tz_add` は `extern "C" { fn tz_add(x: i64, y: i64) -> i64; }` で宣言する。

```sh
tsuzuri build Kernel.tz --emit bitcode -o target/tz/tzkernel.o
ar crs target/tz/libtzkernel.a target/tz/tzkernel.o
RUSTFLAGS="-C linker-plugin-lto -C linker=clang -L native=target/tz -l static=tzkernel" cargo build --release
# Linux: RUSTFLAGS に -C link-arg=-fuse-ld=lld を足す。rustc -vV の LLVM の版がリンカーの LLVM 以下であること
```

## 受け入れ条件

- [x] `tsuzuri build <dir> --emit bitcode` が native で ThinLTO の要約を持つ bitcode を出し（`GLOBALVAL_SUMMARY_BLOCK` が 1 つ）、wasm64 と Windows は診断の表の E2000 になる（wasm32 は Phase 2 で通る）。
- [x] bitcode のすべての `define` が probe した target 属性を持ち（`tests/lto.rs` の stamp の単体と全 fixture の IR）、外部に定義される記号が export と、弱い runtime 記号だけ（`the_c_runtime_is_merged_into_the_bitcode_and_hidden`）。
- [x] 同じ入力・同じ Clang で bitcode が byte 単位で一致する（`bitcode_is_the_same_bytes_every_time`。C ランタイムを含む場合と wasm32 も）。
- [x] `tz_add` がホストのループに inline され（`main` の中の `tz_add` を指す命令が 0）、出力が正しい（`a_c_host_inlines_the_bitcode_but_not_an_object`、`lto/small_call`）。
- [x] `cargo test --locked --test lto` が `17 passed`、`RUST_MIN_STACK=4194304 cargo test --locked --no-fail-fast` が 1473 passed・0 failed。
- [x] 既定の出力が変わらない（baseline の `tsuzuri-before` との比較。「検証」）。
- [x] `lto/small_call` の 3 通りを計測し、環境とともに `docs/benchmarks.md` に記録している。測っていない速さを主張していない。
- [x] 「ドキュメント」の各ファイルを更新し、未検証の組み合わせ（Rust の native・Linux の実行・Windows）をそう書いている。
- [x] GUIDE §10 の完了の定義を満たす（fmt・clippy 1.99・Windows の型検査・回帰テスト 4 つ・文書の検査）。

## 落とし穴

- 属性がないと inline されない: remark `conflicting attributes` が出る。`alwaysinline` を付けたり inline の閾値を変えたりして回避しない（D3）。
- ホストが `-flto`（full）だと、Tsuzuri の ThinLTO module から import せず inline されない（「再現」で確認）。文書と例は必ず `-flto=thin`。
- `.bc` をリンク行に渡すと Clang が一度コンパイルし直す。結果は同じだが、Rust の static library には `.o` の名前で入れる。
- macOS で Homebrew の `lld` を使うと Apple clang の bitcode で `LLVM ERROR: Unsupported stack probing method` になる。Apple の `ld` を使う。
- Xcode には `llvm-link` がない。`/opt/homebrew/opt/llvm@21/bin/llvm-link` を使う。llvm@22・@23 の `llvm-link` の `-S` 出力は Apple clang 21 が
  読めない構文を含み得る（generated `.ll` の既知の差）。版を混ぜない。
- `--internalize` は 2 番目以降のファイルだけに効く。`module.ll` を必ず先頭に置く（逆にすると `tz_*` が internal になり停止条件）。
- `numeric.ll` のグループの `"tune-cpu"="generic"` は置き換わる。意図どおり（通常の object でも Clang の既定 CPU で codegen される）。
  それ以外の鍵（`"no-builtins"`・`memory(...)` など）は触らない。
- `-O0` の bitcode では C ランタイムの関数に Clang が `optnone noinline` を付けるので、リンク時にも inline されない。仕様どおり。
- テスト 4 は driver ではなく `emit_target` の IR を使う（`@malloc` の置換のため）。driver の経路に揃えようとしない。driver の経路はテスト 3・5 が見る。
- 実行ファイルへの LTO では inline 後に `tz_add` の記号が消える。共有ライブラリ（`-dynamiclib`・`-shared`）では `tz_*` は残るのが正しい。
- macOS の remark は `ld: warning: LTO remark:` の行として stderr に出る。警告ではない。
- Windows は `llvm::windows_abi` が足す `define internal i64 @write(...) {` も属性付与の対象になる。未検証なので主張しない。

## 対象外

- WASM の bitcode と `wasm-ld` の LTO（Phase 2）、JIT。
- full LTO 用の bitcode を選ぶ option、コンパイラによる LLVM の版の検査、ビルドキャッシュ（D6・D7）。
- Tsuzuri の bitcode の sanitizer 計装。確保の検査は `live == 0` の追跡 allocator で行う。
- PB01 の事前ビルド済みランタイム（object は LTO に参加しない）、E12 のリンク指定の CLI、`run`・`test` での bitcode。
- PR01 の属性の追加。PR01 が IR に付けた属性は bitcode にもそのまま入るので、このチケットで追加の作業はない。

## 決定事項

### D1: 出力の名前と対象

- 決定: `--emit bitcode`、拡張子 `bc`、`--target native` だけ。
- 理由: 既存の値は小文字 1 語（`llvm` がテキスト IR）で、`llvm-bc`（rustc）の形は既存の命名と合わない。WASM は export・import の検査が別に要る。
- 状態: 実装済み（改訂した D1 のとおり。native と `--target wasm32`、wasm64 と Windows は E2000）

### D2: ThinLTO

- 決定: bitcode は `-flto=thin` で作り、ThinLTO の要約を持たせる。ホストにも `-flto=thin` を求める。
- 理由: full LTO のホストと ThinLTO の module の組では inline されないことを確認した。Rust の linker-plugin LTO も ThinLTO の module を出す。
- 状態: 実装済み（`-flto=thin`。ThinLTO の要約は `llvm-bcanalyzer` で 1 つ確認）

### D3: target 属性の付与

- 決定: 設定された Clang で空の C 関数を IR にし、その `"target-cpu"`・`"target-features"`・`"tune-cpu"` を bitcode の全 `define` に付ける（アルゴリズム）。
- 理由: 属性がないことが inline されない原因だと確認し、付けると inline された。値は同じ Clang・同じ flags で C に付く値と同じなので、C のホストと
  互換で、通常の object の codegen が使う既定 CPU とも一致する。新しい crate や LLVM の API が要らない。
- 状態: 実装済み（改訂した D3 のとおり: `target-cpu`・`tune-cpu` を付け、`--cpu native` と wasm32 の `simd128` だけ `target-features` も付ける。F08 の `@cpu` 版は自分の features を保つ）

### D4: ランタイムの扱い

- 決定: 埋め込みの `.ll` は IR に含まれたまま。C のランタイムは必要なときだけ `-S -emit-llvm` で IR にし、`llvm-link --internalize`
  （`TSUZURI_LLVM_LINK`）で一つにしてから bitcode にする。
- 理由: 旧「未決事項」の既定案（一つの bitcode）を保つ。既存の `merge_debug_ir` の経路を再利用でき、ランタイムの記号が外へ出ない。
- 状態: 実装済み（改訂した D4 のとおり。`llvm-link` は環境変数・配布物・Clang の隣・PATH の順で探す。`merge_debug_ir` の経路は `merge_runtime_ir` に広げた）

### D5: 公開する記号

- 決定: 「生成 IR とランタイム」の表のとおり。外部の定義は `tz_*`・`tsuzuri_alloc`・`tsuzuri_free` だけ。新しい外部記号を足さない。
- 理由: C header・object と同じ公開 ABI を保つ。
- 状態: 実装済み（改訂した D5 のとおり。強い定義は export だけで、`tests/lto.rs` が `llvm-nm` で `tasks` の 37 個の export と一致することを確かめる）

### D6: LLVM の版

- 決定: コンパイラは版を検査しない。版は bitcode の IDENTIFICATION に入り、文書で確認方法と規則（リンカーの LLVM ≥ 全 producer）を示す。
- 理由: 読めるかどうかを決めるのはホストのリンカーで、コンパイラはそれを知らない。rustc の LLVM 22.1.8 の bitcode を Apple の `ld` が読めた例のように、
  版の数字だけでは判定できない。
- 状態: 実装済み（改訂した D6 のとおり。版は検査せず、出力の先頭の magic だけを検査して E2002）

### D7: キャッシュ

- 決定: bitcode はビルドキャッシュを使わない。
- 理由: key に `llvm-link` と probe の結果を足す必要があり、bitcode は頻繁に作る出力ではない。
- 状態: 実装済み（cache の key にも store にも入れない。`clang_identity` も取らない）

### D8: option の意味

- 決定: `-O<n>` は pre-link の最適化の段。`--cpu native`・`--debug-info`・`--trap-info` は object と同じ意味で使える。`--cpu native` の既存の
  診断文は変えない。
- 理由: IR を object と同じ `llvm::emit_native_build` で作るので、意味を分ける理由がない。既存の文を変えるとテストの期待値が変わる。
- 状態: 実装済み（改訂した D8 のとおり。`-Os`・`-Oz`・`--trap-mode return`・`--allocator system|small|host|counting` も通る。`--freestanding`・`--strip`・リンク入力・bare metal・Windows は拒否）

### D9: `llvm-link` のないテスト環境

- 決定: `bitcode_merges_c_runtime_and_internalizes_it` は `TSUZURI_LLVM_LINK` がなければ skip の行を出して成功する。他の 4 テストは Clang だけで動く。
- 理由: Xcode に `llvm-link` がなく、既存のテストも `llvm-link` を要求していない。実装手順の確認コマンドは必ず `TSUZURI_LLVM_LINK` を付けて実行する。
- 状態: 実装済み（変更あり。`llvm-link` は Clang の隣からも探すので、CI の LLVM 21 では見つかる。道具（`llvm-link`・`llvm-nm`・`clang++`・ThinLTO のリンク・`wasm-ld`・Node）がないとき、手元は skip の行を出して成功し、環境変数 `CI` があるときは失敗にする）

### D10: Phase 2（WASM）

- 決定: 「Phase 分割」の設計方針で、人間が求めた場合だけ着手する。
- 理由: `wasm-ld` の LTO と export の組み合わせ、node での import の検査が別に要り、Phase 1 の目的（C/C++/Rust のホスト）には不要。
- 状態: 実装済み（2026-10-17 に人間が求めた。改訂した D10 のとおり。wasm32 だけ。`simd128` は小さな matrix で測り、属性を付けることにした）

## 実装と検証

Phase 1（native の `--emit bitcode`）と Phase 2（wasm32 と `wasm-ld` の LTO）のすべてを、ブランチ `perf/p4-PR08` に実装した。

### コミット

| 作業 | コミット | 内容 |
| --- | --- | --- |
| WP0 | `21f88486` | 着手時の再検討（このチケットだけ） |
| WP1 | `d1b67272` | `Emit::Bitcode`、`output_path`、`validate_bitcode`、CLI（`--emit` の一覧と文、`--help`）、単体テスト、`tests/bindings.mjs` の文 |
| WP2 | `2d0f11b4` | `src/driver/lto.rs`（target 属性の probe と stamp、magic の検査、`llvm-link` の探索）と `tests/lto.rs` の最初の 6 件 |
| WP3 | `099f6b64` | `build_complete` への配線（IR への属性、C ランタイムの `-S -emit-llvm` と `llvm-link --internalize`、`-flto=thin`、cache の対象外）、`scripts/toolchain/clang.rs` の `-flto*` の転送 |
| WP4 | `be261d9e` | `tests/lto.rs` の host のテスト（C・C++ の inline、C ランタイムの結合と隠蔽、CPU kernel・allocator・trap return の往復、決定性、Clang と `llvm-link` の失敗） |
| WP5 | `10551c9d` | `benchmarks/run-lto.mjs`、`benchmarks/lto/small_call.c` |
| WP6 | `d832fe24` | wasm32（`validate_bitcode`、simd128 の属性、`wasm-ld` の LTO と Node のテスト） |
| WP7 | `b29d161e` | 文書（言語リファレンス、`docs/language.md`・`architecture.md`・`benchmarks.md`、README） |
| WP8 | `54f9db5c` | チケットの完了と検証の記録 |
| レビュー 1 | `e2501638` | `--emit bitcode` が `tsuzuri_async_post` を内部にする誤りの修正（`keep_runtime_exports`・`check_kept_exports`）、host のスレッドからの post と `--emit shared` の export との一致のテスト、`docs/language.md`・`native-interop.md`・`architecture.md` の記号の一覧の統一 |
| レビュー 2 | `8557d896` | llvm-link の hint を、C ランタイムを併合したときだけ付ける。wasm32 と C ランタイムのない native のテスト |
| レビュー 3 | `409200bc` | bitcode が object のオプション（`-g`・`--cpu native`・`-O0`・`-Oz`・`--trap-info`）を取ることのテスト |
| レビュー 4 | `47f14422` | 上の修正の記録 |
| CI の指摘 1 | `77d5b44a` | Linux の CI で落ちた C・C++ のホストのテストを、記号表でなく呼び出し命令で見るように直す。`native-interop.md` の更新 |
| CI の指摘 2 | このコミット | 上の修正の記録 |

### 実装したこと

- `--emit bitcode`（拡張子 `.bc`）: native（`--target native` と Windows 以外の `--target TRIPLE`）と `--target wasm32`。IR に target 属性を付け、C ランタイムは IR にして `llvm-link --internalize` で 1 つにまとめ、`clang -x ir -flto=thin -c` で ThinLTO の要約つきの bitcode にする。出力の先頭の magic を検査し、LTO の指定を落とす Clang や launcher は `E2002`。
- 属性: probe（小さな C を同じ Clang で `-S -emit-llvm`）から `target-cpu`・`tune-cpu` を読み、全 `define` に付ける。`--cpu native` と wasm32 の `simd128` は `target-features` も付ける。F08 の `@cpu` 版（自分の `"target-features"` を持つ `define`）は features を保つ。stamp は決定的で冪等。
- 配布物の launcher（`scripts/toolchain/clang.rs`）の `-x ir` の経路が `-flto*` を同梱の Clang に渡す。
- wasm32: C ランタイムも `llvm-link` も使わない。`--wasm-max-memory` は通し、threads・jspi・webgpu・net・WASI・`--wasm-stack-size`・wasm64 は `E2000`。
- 測定の runner `benchmarks/run-lto.mjs`（PX01 の suite `lto`）と、その C のホスト。

### 実装しなかったこと、対象外のまま残したこと

- Windows（`E2000`。緑の Windows CI で確かめてから外す）、wasm64、full LTO の bitcode を選ぶ option、コンパイラによる LLVM の版の検査、ビルドキャッシュ、PB01 の事前ビルド済みランタイムの使用、`run`・`test` での bitcode。
- `scripts/toolchain/smoke.mjs` への bitcode の検査（実行するには配布物全体を作る必要があり、ここでは試せなかった）。`.github/workflows/*` は触っていない。
- `tsuzuri toolchain info` に、Clang の隣の `llvm-link` の探索は出さない（文書に書いた）。

### 測定（`lto/small_call`、n = 10 億、9 標本、4 回）

| 構成 | `object` | `bitcode` | `c-only` |
| --- | --- | --- | --- |
| 中央値の範囲（4 回、ms） | 1020.9〜1163.1 | 548.8〜674.1 | 533.7〜676.0 |
| `main` の中の `tz_add` の命令 | 1 | 0 | 0 |

`object` の最小（991.7〜998.7 ms）は `bitcode` の最大（687.4〜732.5 ms）より大きく、4 回とも範囲が重ならない。`bitcode` と `c-only` は 4 回とも範囲が重なり、差なし。M1 Max、macOS 27.0.1、load average 12〜18 の共有機の目安で、x86-64・Linux・Windows は測っていない（詳細は `docs/benchmarks.md` の `phase4:PR08`）。ビルドは、C ランタイムを使わない `add` で約 50 ms 長く、C ランタイムを使うプログラムでは object より短かった。

### 検証

| 確認 | 結果 |
| --- | --- |
| `RUST_MIN_STACK=4194304 cargo test --locked --no-fail-fast`（`TSUZURI_CLANG` は Homebrew LLVM 21.1.8、`TSUZURI_WASM_LD` は LLD 23.1.1） | 1473 passed、0 failed、1 ignored（測定用）。105 の test binary、29 分 24 秒。うち `tests/lto.rs` が 17 passed、lib が 313、bin が 22 |
| GUIDE §3.1 の回帰テスト 4 つ（既定の stack） | それぞれ `running 1 test`、ok |
| `cargo +1.99.0 fmt --all -- --check`、`cargo +1.99.0 clippy --all-targets --locked -- -D warnings` | 問題なし |
| `cargo +stable check --all-targets --target x86_64-pc-windows-msvc`・`aarch64-pc-windows-msvc` | 警告 0 |
| node の E2E（`target/release/tsuzuri`）: `wasm_memory`、`bindings`、`handoff_tools`、`cache`、`llvm_handoff`、`trap_boundary`、`e2e`、`targets` | すべて exit 0 |
| 既定の出力が変わらない: `benchmarks/check-handoff-equivalence.mjs`（baseline の `tsuzuri-before` と比較） | `--quick` が 139 件、全 project の header・llvm・object・shared・wasm が 705 件で、すべて合格（587 件が完全に一致、118 件は両方とも同じ診断で失敗） |
| 同じく、`target/perf/PR08/work/same-outputs2.sh` の byte 比較（`-O0`・`-Oz`・`-g`・`--trap-mode return`・`--allocator small`・`--cpu native`・`--freestanding` の object、shared、cache ありの object、llvm、header、wasm。7 project） | 98 件が byte 一致、差 0 |
| `node scripts/check-docs.mjs`（native-interop・option・usage・webassembly）、`git diff --check` | 138 links、17 examples、12 native runs。問題なし |
| 同梱の launcher（`scripts/toolchain/clang.rs` を build し、配布物の clang・zig・llvm-link で） | bitcode の magic を確認。Linux の x86-64（glibc）と AArch64（musl）のホストを `zig cc -flto=thin` でリンクして inline を確認（実行はしていない）。macOS のホストは `error: LTO requires using LLD` |
| wasm32 と Rust（rustc 1.96.1、LLVM 22.1.2、`-C linker-plugin-lto`、`rust-lld`） | 手で確認。結果が正しく、import が空で、inline された |

### 起票・再検討との違いと訂正

- Phase 0 の記録の「Apple clang には wasm の backend がなく」は誤りだった。Apple clang 21.0.0 は `wasm32`・`wasm64` の backend を持ち、`tests/lto.rs` の wasm のテストは Apple clang でも通る（`wasm-ld` は LLD 23）。
- wasm32 の `--wasm-feature simd128` は、属性を付けないと `wasm-ld` の LTO が SIMD の命令を落とす（`wasm_simd` の fixture で 0 命令。付けると regular の `--emit wasm` と同じく SIMD が残る）ので、付ける。付けなくても結果は同じ。
- テストの skip: 起票の D9 は skip だけだったが、道具がないとき CI（環境変数 `CI`）では失敗にする。道具の探索は Clang の隣を含む。
- PX01 の variant 名は `[a-z0-9-]+` なので、`c_only` は `c-only`。M1（呼び出し命令の数）は PX01 の指標にないので、`samples.json` と `docs/benchmarks.md` に書いた。
- `--wasm-max-memory` の診断の文に `bitcode` を足した（`src/driver.rs`・`src/main.rs`・`tests/wasm_memory.mjs` の期待値）。
- 同梱の Zig の `zig cc` が `-S` に足す `-c` の警告（`argument unused during compilation: '-c'`）を、IR へ併合する runtime のコンパイルで抑えた（macOS の `-g` の object の経路にも効く。成果物は変わらない）。
- wasm の bitcode の検査は Node.js 20 でも通る（起票は 24）。

### 既知の制限

- Windows は実行していない（`E2000`）。Linux は、このマシンの Docker のコンテナが起動しなかった時点では実行できなかったが、その後の CI（x86-64）と AArch64 のコンテナで実行した（「Linux の CI の指摘への修正」）。x86-64 の `cargo test` をこのマシンで実行する手段はない（エミュレーションが約 14 倍遅い）。
- native の Rust のホスト（macOS）は inline されない。Linux の Rust（AArch64・x86-64）は inline された（「Linux の CI の指摘への修正」）。
- 同梱の Zig の macOS のホストは LTO をリンクできない。Linux の bundle には `llvm-link` がなく、C ランタイムを使うプログラムの bitcode には別の `llvm-link` が要る。
- bitcode を読む LLVM は、書いた LLVM と同じか新しくなければならない。コンパイラは検査しない。
- `-g` の bitcode は、既定の DWARF 5 のホストとのリンクで `Dwarf Version` の警告が出る（ホストに `-gdwarf-4` で消える）。
- PR07（`--profile-*`）と PB05（`--backend fast`）は、`--emit bitcode` を allow-list で拒否する。統合後に組み合わせを確かめる。

### レビューの指摘への修正（2026-10-17）

統合ブランチ（PR07・PB06 を含む）を取り込んだ後の独立したコードレビューが、2 件を指摘した。どちらも直し、網羅の穴の 1 件も埋めた。

**1. `tsuzuri_async_post` が内部になっていた（中）**

- 再現: `tests/fixtures/async_reactor` を `--emit object` と `--emit bitcode` にして `llvm-nm` で比べた。object と `--emit shared` は `tsuzuri_async_post` を外部に出し、bitcode は出さなかった。bitcode の外部は `tz_*`、`tsuzuri_alloc`・`tsuzuri_free`・`tsuzuri_trap_hooks`（W）、`tsuzuri_async_poll`・`tsuzuri_async_complete`（T）だけだった。
- 原因: `tsuzuri_async_post` は C ランタイム（`async.c`、`weak`、default visibility）だけが定義する。`llvm-link <module.ll> <task.o> --internalize` は、2 つ目のファイルだけが定義する記号をすべて内部にする。D5 は公開の集合を `shared_exports` と同じにしたが、そのうち C ランタイムが定義する記号を数えなかった。
- 直し方: 公開 ABI の記号のうち IR が定義しないもの（`shared_exports` の一覧から求める）に、まとめる前の IR で `declare`（すでに宣言があれば足さない）と `@llvm.used` の一覧を足す（`lto::keep_runtime_exports`）。`llvm-link --internalize` が残す名前の一覧は `@llvm.used` だけで（`llvm.compiler.used` は読まれない）、配布物に `opt` がないので `opt -passes=internalize` は選べない。`async.c` を変えると既定の object・事前ビルドの runtime・cache の key が変わるので選ばない。`llvm-link` の後に `lto::check_kept_exports` が、まとめた IR でそれらが外部・default visibility の定義のままか検査し、違えば `E2002`（`@llvm.used` を読まない `llvm-link` への備え）。
- 全部の記号を調べた: C ランタイム（`os.c`・`net.c`・`async.c`・`gpu.c`・`trap.c`・`task.c`・`cpu.c`・`io.c`・`arguments.c` をドライバーと同じく 1 つにして、`-DTZ_TRAP_BOUNDARY -pthread` で IR にした）で、internal でも hidden でもない定義は、`tsuzuri_async_post`（`weak`）と、生成された `main` が呼ぶ内部の hook の `tsuzuri_arguments` だけだった。ほかは `weak hidden` か `static`。`shared_exports` の一覧は、IR が定義する `tz_*`・`tsuzuri_try_*`・`tsuzuri_alloc`・`tsuzuri_alloc_stats`・`tsuzuri_async_poll`・`tsuzuri_async_complete`・`tsuzuri_free`・`tsuzuri_main` と、C ランタイムが定義する `tsuzuri_async_post` だから、失われていたのは `tsuzuri_async_post` だけである。`tsuzuri_arguments` は ABI ではないので内部のまま。
- 見えなかった理由: 全 project の `--emit shared` と `--emit bitcode` の突き合わせ（直す前）は、80 件が一致、35 件は共有ライブラリを作れず、欠けは 0 件だった。reactor を使う project は共有ライブラリにならない。object と比べて初めて見えた。直した後は、114 project のうち object を作れる 102 件のすべてが bitcode も作れる。
- 代償: `@llvm.used` に載るので、ホストのリンクの `--gc-sections`・`-dead_strip` でも `tsuzuri_async_post`（数百バイト）は残る。
- テスト（`tests/lto.rs`）: `a_host_thread_posts_to_the_reactor_of_the_bitcode`（`tsuzuri_async_post`・`poll`・`complete` が外部で `tsuzuri_async_wait` が内部。C のホストの別スレッドが post して、object と bitcode のどちらでも `40050 0`）と、`the_bitcode_defines_every_symbol_that_a_shared_library_exports`（async・hello・counting allocator・trap return の 4 通りで、`--emit shared` の export がすべて bitcode の外部定義にある）。直す前に失敗することを確かめた。単体は `driver::lto::tests` に 5 件。
- 文書: `docs/language.md`、`native-interop.md` の「何が入るか」、`architecture.md` の手順 (3) が、同じ一覧を書く。強い定義は `tz_*`・`tsuzuri_try_*`・`tsuzuri_alloc_stats`・`tsuzuri_main`・`tsuzuri_async_poll`・`tsuzuri_async_complete`、弱い定義は `tsuzuri_alloc`・`tsuzuri_free`・`tsuzuri_trap_hooks`・`tz_soft_*`・`tz_small_*`・`tsuzuri_async_post`、C ランタイムのそれ以外は内部。

**2. llvm-link の hint（低）**

- 最後の Clang の失敗に「IR を llvm-link がまとめた…」の hint を、`merge_runtime_ir && options.emit == Emit::Bitcode` で足していた。bitcode では `merge_runtime_ir` がいつも真なので、`llvm-link` が走らない wasm32 と、C ランタイムのない native にも出ていた。`native_runtime && options.emit == Emit::Bitcode` にした。
- テスト: `a_failed_final_compile_names_llvm_link_only_when_it_ran`。`-flto=thin` の compile だけを失敗させる Clang の wrapper を `TSUZURI_CLANG` にして、native の `add` と wasm32 の `add` は `llvm-link` を名指さず、`tests/fixtures/tasks`（C ランタイムあり）は名指すことを確かめる。

**3. object と同じオプション（網羅）**

- `bitcode_takes_the_options_of_an_object`: `-g`・`--cpu native`・`-O0`・`-Oz`・`--trap-info` の 5 通りの bitcode を C のホストと `-flto=thin` でリンクして、結果（`100 42 6`）を確かめる。どれも最初から通り、直す必要はなかった。

| 確認（修正後） | 結果 |
| --- | --- |
| `cargo test --locked --test lto` | 21 passed（debug build、約 100 秒）。直す前は新しい 3 件が失敗 |
| `cargo test --locked --lib driver::`、`--bins` | 65 passed（うち `driver::lto::tests` が 10）、29 passed |
| `cargo +1.99.0 fmt --all -- --check`、`clippy --all-targets --locked -- -D warnings`（各コミットの tree でも）、`cargo +stable check --all-targets --target x86_64-pc-windows-msvc`・`aarch64-pc-windows-msvc` | 問題なし、警告 0 |
| node の E2E（release build）: `wasm_memory`、`bindings`、`handoff_tools`、`cache`、`llvm_handoff`、`trap_boundary`、`e2e`、`targets`、`pgo`（14 sections）、`server` | すべて exit 0 |
| `node benchmarks/run-lto.mjs target/release/tsuzuri --quick` | exit 0（`tz_add` の測定の正しさ） |
| `node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/native-interop.md`、`git diff --check` | 28 links、7 examples、4 native runs。問題なし |

Linux・Windows は、ここでも実行していない。

### Linux の CI の指摘への修正（2026-10-19）

統合ブランチの最初の CI（x86-64 Linux、ubuntu-24.04、apt.llvm.org の clang・lld・llvm 21）で、`tests/lto.rs` の 2 件が落ちた。ほかの 19 件は通った。

- 症状: `a_c_host_inlines_the_bitcode_but_not_an_object`（`tests/lto.rs:846`）と `a_cpp_host_inlines_the_bitcode_too`（`:885`）が `!symbols(&nm, &inlined).iter().any(|(_, name)| name == "tz_add")` で失敗した。リンクと実行（`4999950000`）はその前の assert を通っている。
- 再現: Docker の AArch64 のコンテナ（Ubuntu の clang・lld・llvm 21.1.8、rustc 1.93.1）で、直す前のテストが同じ 2 か所で同じ assert により失敗した（記号表に `T tz_add`）。x86-64 は、エミュレーションのコンテナで、x86-64 向けに作った bitcode を同じ Clang・lld でリンクして同じ結果を得た（`T tz_add`）。
- 原因: テストの前提が誤りだった。起票のテスト計画は「`nm <exe>` に `tz_add` がない」を inline の証拠にしたが、これは macOS のリンカーの振る舞いである。ELF の lld は、`main` から呼び出しが消えても、呼ばれなくなった `tz_add` を `GLOBAL DEFAULT` の関数として実行ファイルに残す（`llvm-readelf -s`）。`-Wl,--gc-sections` を付けると消える（AArch64 と x86-64 で確認）。macOS は LTO の段階で落とす（`-Wl,-object_path_lto` で取った LTO のオブジェクトに `tz_add` がない）。ホストの呼び出しは両方で消えている。AArch64 の `main` は `mov`・`movk` で定数を作って `bl printf` だけ、x86-64 の `main` は `movabsq $0x12a052eb0` と `printf` の呼び出しだけである。object のリンクは `bl 0x7fc <tz_add>`・`callq 0x1180 <tz_add>` を持つ。
- コンパイラは直さない: 公開関数は default visibility の外部定義で、実行ファイルの記号表に残すかどうかは、ホストのリンクの選択（`--gc-sections`、`-dead_strip`、version script）である。bitcode の内容に誤りはない。
- 直し方（`tests/lto.rs` だけ）: 2 件とも、`llvm-objdump -d` の出力で `tz_add` を指す命令（定義の先頭の行 `<tz_add>:` を除く）を数える `instructions_naming` を足し、object のリンクは 1 行以上、bitcode のリンクは 0 行を assert する（残った行を失敗の文に出す）。C のホストのテストには、リンカーが呼ばれない定義を落とす option（ELF は `-Wl,--gc-sections`、Mach-O は `-Wl,-dead_strip`）を付けたリンクで記号表に `tz_add` がないことも足し、文書の「消える」を CI で支える。wasm のテストは、これまでどおり `disassembly_matches`（変えていない）。`benchmarks/run-lto.mjs` の `main` の中の `tz_add` を数える検査は、もともと呼び出し命令で見ていて、Linux の AArch64 でも object 1・bitcode 0・c-only 0 になる（変更なし）。
- 文書: `native-interop.md` の「`llvm-nm host` に `tz_add` がなく」を、`main` に呼び出しがないこと（`llvm-objdump -d host`）と、記号表の違い（macOS は消え、Linux の lld は残り、`--gc-sections` で消える）に直した。「確かめた範囲」の Linux の行を、CI（x86-64）と AArch64 のコンテナの結果に更新した。
- 見つけたこと（直していない）: Ubuntu の Clang 21 の x86-64 は既定の triple が `x86_64-pc-linux-gnu` で、bitcode は `x86_64-unknown-linux-gnu` なので、ホストのビルドごとに `warning: overriding the module target triple with x86_64-pc-linux-gnu [-Woverride-module]` が出る（結果は正しい。`-Wno-override-module` で消える）。`.bc` を `-flto=thin` で渡すと Clang のドライバーがもう一度フロントエンドを通すのが原因である。macOS と AArch64 の Ubuntu では綴りが同じなので出ない。文書の「ホストのビルド」に書いた。コンパイラが Clang の既定の triple を IR に書く案は、別の変更にする。
- native の Rust（Linux）を確かめた（手で、テストにはしていない）: AArch64 と x86-64 のコンテナ（rustc 1.93.1 の LLVM 21.1.8 と、同じ Clang・lld 21.1.8）で、`rustc -O -C linker-plugin-lto -C linker=clang -C link-arg=-fuse-ld=lld -C link-arg=-flto=thin -C link-arg=add.bc` が、ホストの `main` のループを定数にした（`4999950000`）。`-flto=thin` を外すと、リンクはできて inline されない。x86-64 の Rust の object のリンクは、外部関数を GOT 経由で呼ぶので、`tz_add` を名指す命令がない（C のホストの `callq 0x1180 <tz_add>` とは違う）。そのため Rust には、命令の有無でなく `main` の形で見た。

| 確認（CI の指摘の修正後） | 結果 |
| --- | --- |
| AArch64 の Linux のコンテナ（`docker run … tz-pb05-linux-arm64-dev cargo test --locked --test lto -- --nocapture`、`RUST_MIN_STACK=4194304`） | 21 passed、skip なし（ビルド 49 秒、テスト 88 秒）。直す前のテスト（`git archive HEAD`）は同じコンテナで 2 件が失敗 |
| 同じコンテナで `node benchmarks/run-lto.mjs /target/debug/tsuzuri --quick`（GNU time は apt で足した） | `tz_add in main` が object 1、bitcode 0、c-only 0 |
| x86-64 のコンテナ（`--platform linux/amd64`、エミュレーション）で、x86-64 向けの bitcode と object を C・C++ のホストとリンク | 新しいテストと同じ判定: object は 1 行（`callq 0x1180 <tz_add>`）、bitcode は C・C++ とも 0 行（記号表には `T tz_add`）、`--gc-sections` で 0 行・記号なし。出力は `4999950000`。x86-64 の `cargo test` 自体は、エミュレーションが約 14 倍遅く（C の compile の測定）、実行していない |
| macOS（Homebrew LLVM 21.1.8）`cargo test --locked --test lto` | 21 passed（74 秒） |
| `node benchmarks/run-lto.mjs target/debug/tsuzuri --quick`（macOS） | object 1、bitcode 0、c-only 0 |
| `cargo +1.99.0 fmt --all -- --check`、`clippy --all-targets --locked -- -D warnings`、`cargo +stable check --all-targets --target x86_64-pc-windows-msvc`・`aarch64-pc-windows-msvc` | 問題なし、警告 0 |
| `node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/native-interop.md`、`git diff --check` | 29 links、7 examples、4 native runs。問題なし |
