# W37: 同梱の `tsuzuri-clang` が `-O0` の C に UBSan の実行時の参照を入れ、別のリンカーで使えない object になる

| 項目 | 内容 |
| --- | --- |
| ID | W37 |
| 優先度 | P3（修正。言語の意味は変えない。ただし、同梱ツールチェーンの既定の水準で失敗する経路がある。`tsuzuri test --target wasm32`（`test` の既定は `-O0`）の `Channel`、`--wasm-host wasi -O0`、`tsuzuri-clang -c` で作ったホストの object を既定の `-O3` の `build --link` に渡すとき。「現状」の C1〜C3 と B1） |
| 規模 | S（launcher の純粋関数 1 つとその呼び出し、`tests/targets.rs` の単体テスト、`scripts/toolchain/smoke.mjs` の段、文書） |
| 依存 | – |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。W01〜W30 の起票中に見つけ、利用者の依頼「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」に当たるので起票。HEAD `fa40eede` で確認） |
| 承認 | 不要（既定案の範囲。Tsuzuri の言語の意味、生成 IR、診断、フラグ、環境変数は変えない。変わるのは、同梱の launcher で `-O0` 相当にコンパイルした C の UB の検査に当たったときの終わり方だけ（Zig の UBSan のランタイムのメッセージと終了コード 3 → トラップ命令）。台帳は [GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30)） |
| 手本にする既存実装 | `scripts/toolchain/clang.rs` の `is_dropped`（Zig の振る舞いに合わせて引数を直す小さな関数）と `resolve`（`tests/targets.rs` が `#[path = "../scripts/toolchain/clang.rs"] mod shim` で単体テストする純粋関数）、`scripts/toolchain/smoke.mjs`（同梱ツールチェーンだけで build・test・実行を確かめる段） |
| 主な影響ファイル | `scripts/toolchain/clang.rs`、`tests/targets.rs`、`scripts/toolchain/smoke.mjs`、`_tsuzuri/language-reference/compiler/platforms.md`、`_tsuzuri/language-reference/compiler/native-interop.md`、`docs/architecture.md` |

## 実装者向け早見

- 範囲: 同梱の launcher（`scripts/toolchain/clang.rs`）が `zig cc` を起動する枝で、次の 3 つを満たす実行にだけ `-fsanitize-trap=undefined -fno-sanitize=function` を足す。(1) コンパイルだけの実行（`-c` か `-S` を含む）。(2) 最後の `-O` で始まる引数が無いか、`-O0`・`-Og`・`-O` である（Zig 0.16.0 が Debug の水準として UBSan を入れる水準）。(3) `-fsanitize` か `-fno-sanitize` で始まる引数を 1 つも含まない。
- 効果: 今と同じ 17 種の UB の検査を残したまま、`__ubsan_handle_*` の参照がなくなる。検査に当たると、メッセージなしのトラップ命令で終わる。`-O1` 以上の出力、リンクの実行、利用者が sanitizer を指定した実行は 1 byte も変えない。
- 最初に読む: 「現状」の A・B・C・E の表 → `scripts/toolchain/clang.rs` の `main` の `zig cc` の枝（`command.args(["cc", "-target", &zig_target])`）→ 「設計」。
- 最初に書くテスト: `tests/targets.rs` の `the_bundled_launcher_traps_zig_debug_ubsan`（`shim::ubsan_trap` の表）。
- 触ってはいけない: `-x ir` の経路（LLVM Clang に渡す引数の選別）、ARM64 Windows の llvm-mingw の枝、`resolve`・`NATIVE_TARGETS`、`src/` のすべて。
- 完了の見分け方: 新しい同梱ツールチェーンで、次の 2 行目が成功して終了コード 42 を返し、3 行目が成功する（今はどちらも `E2002`）。

```sh
tsuzuri-clang -c host.c -o host.o                         # -O なし（Zig の Debug）
tsuzuri build app --link host.o -o app.exe && ./app.exe   # 既定の -O3。今は lld-link: undefined symbol: __ubsan_handle_add_overflow
tsuzuri test wtest --target wasm32                         # Channel を使う test。今は wasm-ld: undefined symbol: __ubsan_handle_type_mismatch_v1
```

## 目的

同梱の `tsuzuri-clang` は、C を `zig cc` でコンパイルする。Zig は、最適化の指定がないか `-O0`・`-Og`・`-O` のとき（Zig の Debug）、C に UBSan を入れる。その検査に当たったときに呼ぶ関数（`__ubsan_handle_*`）は Zig 自身の UBSan のランタイムにあり、そのランタイムは **Debug の水準で `zig cc` がリンクするときだけ**入る。そのため、`-O0` 相当でコンパイルした object は次のどれでもリンクできない。

- `-O1` 以上でリンクする `zig cc`（`tsuzuri build` の既定の `-O3` を含む）。
- `wasm-ld`（同梱の WebAssembly のリンカー）。
- MSVC の `link.exe`、`lld-link`、システムの Clang・GCC。

この影響は、利用者が `tsuzuri-clang -c` で作るホストの object だけにとどまらない。コンパイラ自身が `-O0` でコンパイルするランタイムの C（WASI・`tzroutine-single`・Linux の `--emit object` に埋め込むランタイム・配布物の `native-c-*-O0.o`）も、同じ参照を持つ。その結果、wasm32 の `-O0` の build と `test` が失敗し、Linux の `-O0` の object はホストのリンクで未定義シンボルを残す。

このチケットでは、launcher が Zig の Debug の UBSan をトラップの形にする。検査の種類は今と同じに保ち、どの object も Zig のランタイムに頼らないようにする。`-O1` 以上の出力は変えない。

関連: [W01](_completed/W01-callback-extern-temporary-string.md)（その再現の注記がこの問題を `-fno-sanitize=undefined` で避けている）、[W03](_completed/W03-windows-interop-doc-fixes.md)（リンクの失敗の hint と、リンク入力の文書）、[G15](_completed/G15-platform-targets.md)（launcher の target の写像）、[PB01](../_perfs/_completed/PB01-prebuilt-runtime.md)（配布物のランタイム object）、[E12](_completed/E12-ffi-extensions.md) D6（リンク入力）。

## 着手条件と停止条件

### 着手条件

- `node scripts/repo-kb.mjs brief W37` を読む。`scripts/toolchain/` を変えるので、AGENTS.md に従って `.github/instructions/ci.instructions.md` を読む（同梱の Windows x64 の `zig cc` が `-O1` から `NDEBUG` を定義する、という教訓も同じ種類の Zig の既定の違いである）。
- `grep -n "zigVersion = " scripts/toolchain/bundle.mjs` が `0.16.0` であること。違えば、「現状」の A4・E3 を新しい Zig でもう一度確かめる（停止条件 S2）。
- 「現状」の A1・B1・C2・C3 を HEAD の同梱ツールチェーン（VS Code の拡張の 0.1.6、または `node scripts/toolchain/bundle.mjs` で作ったもの）で再現する。結果が違えば、このチケットを直してから始める。
- `grep -n 'command.args(\["cc", "-target", &zig_target\]);' scripts/toolchain/clang.rs` が 1 件当たること。

### 停止条件

- **S1** 新しい launcher で `-O1` 以上（`-O1`〜`-O4`・`-Os`・`-Oz`・`-Ofast`）のコンパイルの出力が 1 byte でも変わる（実装手順 3 の配布物のランタイム object の比較、または smoke のバイト比較）。Zig の解釈が想定と違うので、止めて報告する。
- **S2** 新しい launcher の `-O0`・`-Og` のコンパイルで、cc1 の `-fsanitize=` の種類が、今の既定（「現状」A4 の 17 種）と違う target がある（Zig の版を上げたときなど）。target ごとに引数を変える即興をせず、止めて報告する。
- **S3** トラップにしたランタイムの C が、smoke の段や同梱ツールチェーンで回す suite（`vscode.yml` の `tests/net.mjs` など）でトラップする。今は Zig のランタイムがメッセージを出して終わる UB が、本当にあったことになる。検査を外さず、C の誤りとして報告する（W37 の範囲は、検査に当たったときの反応だけ）。
- **S4** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

再現は Git Bash で、開発版のコンパイラ（`target/debug/tsuzuri.exe`）と VS Code の拡張に同梱のツールチェーン（0.1.6、Zig 0.16.0、LLVM 21.1.8）を使って行った。作業は `target/scratch/w-w37/` で行い、確認後に消した。

```sh
T=$HOME/.vscode/extensions/tmidorikawa.tsuzuri-0.1.6-win32-x64/toolchain
NM="/c/Program Files/LLVM/bin/llvm-nm.exe"          # 配布物には llvm-nm が無い
TZ=../../debug/tsuzuri.exe
# host.c: int64_t add_host(int64_t a, int64_t b) { return a + b; }
#         int32_t shift_host(int32_t a, int32_t n) { return a << n; }
#         int32_t index_host(const int32_t *p, int64_t i) { return p[i]; }
# app/Main.tz: @trusted
#              extern "add_host" def add_host :: i64 -> i64 -> i64
#              def main :: unit -> i32 = \() ->
#                  add_host 40 2 as i32
$T/bin/tsuzuri-clang.exe -c host.c -o host.o && "$NM" -u host.o
TSUZURI_CLANG=$T/bin/tsuzuri-clang.exe $TZ build app --link host.o -o app.exe
```

### コード

- `scripts/toolchain/clang.rs` の `main`: `--target=` を取り除き、`is_dropped`（`-fuse-ld=lld`・`-Werror=incompatible-sysroot`）を捨てる。それ以外の引数はそのまま `zig cc -target <Zig の target>` に渡す（`Command::new(zig)` の枝）。sanitizer の引数は扱わない。`-x ir` の経路は IR を同梱の LLVM Clang でコンパイルし、渡すのは `-O*`・`-W*`・`-m*`・`-g`・`-fPIC`・`-flto*`・`-ffunction-sections`・`-fdata-sections` だけなので、IR のコンパイルに UBSan は入らない。ARM64 Windows のホストの native は Zig ではなく、同梱の LLVM Clang と llvm-mingw の sysroot を使う。
- `src/driver.rs` の `build_complete`: ランタイムの C（`runtime.c` に連結した `os.c`・`net.c`・`trap.c`・`cpu.c`・`io.c`・`arguments.c`・tzroutine）を `-std=c11 -c`（macOS の `-g` の object と `--emit bitcode` では `-S -emit-llvm`）と `options.optimization.flag()` でコンパイルする。WASI（`os-wasi.c`）、`tzroutine-single`、JSPI、`--wasm-feature threads` の C も、同じく `options.optimization.flag()` でコンパイルする。`-g` のない `stack.c`・`backtrace.c` は、リンクの実行の中で `-x c -std=c11` としてコンパイルする（`-g` があれば別の object を `-O1` で作る）。キャッシュが有効なら `compile_runtime_object` が、配布物の object → ランタイムのキャッシュ → その場のコンパイルの順に探す。
- `src/test_runner.rs` の `build_debug_runner` はランタイムの C を `-x c -std=c11 -c <最適化>` でコンパイルし、`compile_native_runner` はリンクの中で `-x c` にする。
- 既定の水準: `src/main.rs` の `parse_arguments` は、`build`・`run` を `-O3`、`test` と `--backend fast` を `-O0` にする（`repl` も `-O0`）。
- `scripts/toolchain/runtime.mjs` の `bundleRuntime` は、配布物の `native-c`（`cpu.c`・`io.c`）を、同梱の launcher で `-std=c11 -c --target=<arch>-pc-windows-msvc|-fPIC -O0〜-O3 -ffunction-sections -fdata-sections` にしてコンパイルする。manifest の `tools` には launcher の SHA-256 を書く。`src/runtime_cache.rs` の `prebuilt` は、その SHA-256 が今の launcher と一致するときだけ配布物の object を使う。`src/cache.rs` の `read_identity`・`bundled_identity` は、キャッシュの鍵（`tool-binary`）に launcher の digest を入れる。したがって launcher を変えると、古い object は配布物からもキャッシュからも使われない。

### A. Zig がいつ UBSan を入れるか（`tsuzuri-clang -c host.c`）

| # | 試したこと | 結果 |
| --- | --- | --- |
| A1 | `-O` なし、`-O0`、`-Og`、`-O`（引数のない `-O`） | どれも `__ubsan_handle_add_overflow`・`__ubsan_handle_shift_out_of_bounds`・`__ubsan_handle_pointer_overflow`・`__ubsan_handle_type_mismatch_v1` の 4 つが未定義 |
| A2 | `-O1`・`-O2`・`-O3`・`-Os`・`-Oz`・`-Ofast`・`-O4` | 参照なし。`llvm-objdump -d` に `ud1`・`ud2` もない（検査なし） |
| A3 | 引数の順 | 最後の `-O` が勝つ。`-O0 -O2` は参照なし、`-O2 -O0` と `-O2 -Og` は 4 つ。引数のない `-O` は、Clang の `-O1` と違い、Zig 0.16.0 では Debug になる |
| A4 | `-O0 -v` の cc1 の引数（新しい `ZIG_GLOBAL_CACHE_DIR`・`ZIG_LOCAL_CACHE_DIR` で。Zig は自分のキャッシュに当たると cc1 の行を出さない） | `-fsanitize=alignment,array-bounds,bool,builtin,enum,float-cast-overflow,integer-divide-by-zero,nonnull-attribute,null,pointer-overflow,return,returns-nonnull-attribute,shift-base,shift-exponent,signed-integer-overflow,unreachable,vla-bound`（17 種）と、そのうち 15 種の `-fsanitize-recover=`。`function` は含まない |
| A5 | A4 を `NATIVE_TARGETS` の 9 個（`arm64-apple-macosx12.0.0`、Linux の glibc・musl の x86-64・AArch64、`*-w64-windows-gnu`、`*-pc-windows-msvc`）、`wasm32-unknown-unknown`、`--target=` なし（ホストの既定）の `-O0`・`-Og` で | どれも A4 と同じ 17 種。wasm32 も `__ubsan_handle_*` を参照する |
| A6 | 同梱の LLVM Clang そのもの（`$T/bin/clang.exe -target aarch64-w64-windows-gnu -O0 -c`。ARM64 Windows の配布物の native の枝と同じ Clang の既定） | 参照なし。`-fsanitize-trap=undefined` だけを足しても object は同じ |

### B. ホストの object のリンク（A1 の `host.o`）

| # | 試したこと | 結果 |
| --- | --- | --- |
| B1 | `TSUZURI_CLANG=$T/bin/tsuzuri-clang.exe $TZ build app --link host.o`（既定の `-O3`） | `E2002`。`lld-link: error: undefined symbol: __ubsan_handle_add_overflow` と `shift_out_of_bounds`・`pointer_overflow`。同梱ツールチェーンどうしでも通らない |
| B2 | 同じ `build` に `-O0` | 成功して 42（Zig の Debug のリンクが UBSan のランタイムを入れる） |
| B3 | `tsuzuri-clang` で object だけをリンク（`host.o` と `-O2` の `callmain.o`） | `-O2` は B1 と同じ未定義。`-O0` と `-O` なしは成功（実行ファイルは 780,800 バイト） |
| B4 | MSVC（Developer PowerShell、`TSUZURI_CLANG="C:\Program Files\LLVM\bin\clang.exe"`）の `build app --link host.o` | `E2002`。`lld-link: error: undefined symbol: __ubsan_handle_add_overflow` ほか |
| B5 | `link.exe /nologo /dll /noentry host.o` | `LNK2019: unresolved external symbol __ubsan_handle_add_overflow` ほか 4 つと `LNK1120` |
| B6 | `-O2 -c host.c` の object で B1・B4 | どちらも成功して 42 |

### C. コンパイラ自身のランタイムの C

| # | 試したこと（`TSUZURI_CLANG`・`TSUZURI_WASM_LD` は同梱） | 結果 |
| --- | --- | --- |
| C1 | `do! IO.writeln "hi"` の `main` を `build --target wasm32 --wasm-host wasi -O0 --no-cache` | `E2002`。`wasm-ld: error: ...wasi.o: undefined symbol: __ubsan_handle_shift_out_of_bounds`（ほかに `pointer_overflow`・`sub_overflow`・`add_overflow`・`builtin_unreachable`）。`-O1`・`-O3` は成功 |
| C2 | `Channel.make`・`send`・`recv` だけの export（起動しないので `tzroutine-single.o`）を `build --target wasm32 -O0 --no-cache` | `E2002`。`wasm-ld: error: ...tzroutine-single.o: undefined symbol: __ubsan_handle_type_mismatch_v1`。`-O3` は成功 |
| C3 | C2 の関数を使う `test "channel" = assert (answer 3 == 3)` を `tsuzuri test --target wasm32`（既定の `-O0`） | C2 と同じ `E2002`。`-O3` は `1 passed`。native の `test`（`-O0`）は通る |
| C4 | IO を使う `export def greet` を `build --target x86_64-unknown-linux-gnu -O0 --emit object --no-cache`（Windows のホストから同梱の Zig で） | object は `__ubsan_handle_*` を 10 個参照する（`add_overflow`・`builtin_unreachable`・`load_invalid_value` など）。musl の target も 9 個。`-O3` は 0 個、`--emit llvm` の IR は 0 個。Linux のホストで、実際に gcc・システムの Clang でリンクして失敗することは確かめていない（未確認。未定義シンボルからの推論） |
| C5 | 配布物 0.1.6 の `share/runtime/native-c-{1..7}-O0.o` | 5〜11 個を参照する。`-O1`〜`-O3` の `native-c` と、IR から作る `numeric-*`・`math-*` は 0 個 |
| C6 | C1 の `main` を native で `build -O0`、`test -O0` | 成功する（Zig の Debug のリンクがランタイムを入れる。実行ファイルは 936,448 バイト） |
| C7 | `--emit bitcode -O0` のランタイム入り（Linux の target） | このホストの配布物に `llvm-link` が無く、`E2002`（`cannot execute 'llvm-link'`）で確かめられなかった。ランタイムを `-S -emit-llvm` で IR にする経路は C4 と同じ Zig の Debug のコンパイルなので、併合した IR も `__ubsan_handle_*` を呼ぶはずである（未確認） |

### D. UB に当たったときの今の振る舞い

`volatile int32_t a = INT32_MAX, b = 1; int32_t c = a + b;` の後に `printf` する `ub.c` で確かめた。

| # | 試したこと | 結果 |
| --- | --- | --- |
| D1 | `tsuzuri-clang -O0 ub.c -o ub.exe`（1 回の実行でコンパイルとリンク） | `thread ... panic: signed integer overflow: 2147483647 + 1 cannot be represented in type 'int32_t'` とスタックの表示。`printf` の行は出ず、終了コード 3（`abort`） |
| D2 | `-O0 -fsanitize-trap=undefined` を足す | メッセージなしで終了コード `0xC000001D`（PowerShell の `$LASTEXITCODE`）。実行ファイルは 188,416 バイト（D1 は 779,264） |

### E. 直し方の候補を確かめた結果

| # | 試したこと | 結果 |
| --- | --- | --- |
| E1 | `-O0 -fsanitize-trap=undefined -c host.c` | 参照 0 個、`ud1l` が 4 つ（検査は残る）。ただし cc1 の種類に `function` が加わる（18 種）。各関数の前に `-fsanitize=function` の印（`fe ca 05 c1`）が置かれる |
| E2 | `-O2 -fsanitize-trap=undefined -c host.c` | **検査が増える**。`ud1l` が 4 つ入り、cc1 の種類は `function` と `object-size` を加えた 19 種。Zig はこの引数を「UBSan をトラップの形で有効にする」と読む（A6 の LLVM Clang では何も変わらない）。無条件に足すと、`-O1` 以上のすべての C と配布物の `-O1`〜`-O3` の object が変わる |
| E3 | `-O0 -fsanitize-trap=undefined -fno-sanitize=function` | cc1 の `-fsanitize=` と `-fsanitize-trap=` が A4 と同じ 17 種で、`-fsanitize-recover=` はない。参照 0 個。A5 の 11 通り（`-O0`・`-Og`）すべてで同じ |
| E4 | `-O0 -fno-sanitize=undefined` | 検査なし、参照 0 個 |
| E5 | リンクの実行に足す（`tsuzuri-clang -O0 -fsanitize-trap=undefined host.o callmain.o`。`host.o` は A1 のもの） | 失敗する（Zig は UBSan のランタイムを入れなくなり、A1 の参照が未定義のまま）。`-fno-sanitize=undefined` でも同じ。今 B2・B3 で通る組み合わせを壊す |
| E6 | `-E`、`-dM -E -x c -`、`--version` に足す | 出力は同じ |
| E7 | `src/driver/lto.rs` の `probe_target_attributes` と同じ `-std=c11 -S -emit-llvm --target=<Linux・macOS の target> -fPIC probe.c` に E3 を足す | `@tsuzuri_probe` の属性の `target-cpu`・`tune-cpu`・`target-features` は同じ |

### F. 試作（`target/scratch/w-w37/` の中だけ。確認後に消した）

`scripts/toolchain/clang.rs` を写し、「設計」の `ubsan_trap` を足して `rustc --edition=2024 -O` で作った launcher を、`TSUZURI_ZIG=$T/zig/zig.exe TSUZURI_LLVM_CLANG=$T/bin/clang.exe` で同梱の Zig と Clang に向けた。

| # | 試したこと | 結果 |
| --- | --- | --- |
| F1 | 試作の `-c host.c`（`-O` なし）の object を、0.1.6 の launcher の B1（既定の `-O3`）に渡す | 参照 0 個。成功して 42 |
| F2 | 試作と 0.1.6 の launcher の `-O1`・`-O2`・`-O3`・`-Os`・`-Oz`・`-Ofast` の `-c host.c`、`-O0 -E` | どれも 1 byte も違わない（`-O0 -c` だけが違う） |
| F3 | 試作を `TSUZURI_CLANG` にして C1（ビルドして `node:wasi` で実行）、C2、C3、C4 | C1 は `hi` を出して終了コード 0。C2 は成功。C3 は `1 passed`。C4 の object の参照は 0 個 |
| F4 | 試作で native の `build -O0 --no-cache` と `test`（`-O0`） | どちらも成功する |
| F5 | 試作で `ub.c` を `-O0 -c` にし、`-O2` でリンクして実行 | `0xC000001D`（D2 と同じ）。1 回の実行でコンパイルとリンクをする D1 の形は、試作でも今と同じ（メッセージと終了コード 3） |
| F6 | F1 の object を、Developer PowerShell で `link.exe /nologo /dll /noentry host.o` と、`TSUZURI_CLANG` を LLVM の `clang.exe` にした `build app --link host.o --no-cache` に渡す（B4・B5 の逆） | `link.exe` は終了コード 0。`build` は成功して 42 |
| F7 | 試作の `-O2 -c host.c` と、同じディレクトリでの `zig cc -target x86_64-windows-gnu -O2 -c host.c` | 1 byte も違わない。試作の `-O0 -c` は、`zig cc -target x86_64-windows-gnu -O0 -fsanitize-trap=undefined -fno-sanitize=function -c` と 1 byte も違わない |

## 変えてはいけない意味

- `-O1` 以上（`-O1`〜`-O4`・`-Os`・`-Oz`・`-Ofast`）の C のコンパイルの出力は、1 byte も変えない。配布物の `-O1`〜`-O3` の object も変えない。
- 検査の種類は今の 17 種のまま（A4）。変わるのは、検査に当たったときの反応（Zig のランタイムの関数を呼ぶ → トラップ命令）だけ。
- リンクの実行（`-c`・`-S` を含まない実行）の引数は変えない。今リンクできる組み合わせ（B2・B3 の `-O0` のリンク、1 回の実行でコンパイルとリンクをする D1）は、今と同じに動く。
- 利用者が `-fsanitize*`・`-fno-sanitize*` を 1 つでも渡した実行の引数は変えない（W01 の再現の注記の `-fno-sanitize=undefined` はそのまま効く）。
- `-x ir` の経路（同梱の LLVM Clang への引数の選別）、ARM64 Windows のホストの llvm-mingw の枝、`resolve`・`NATIVE_TARGETS`・`is_dropped` の挙動は変えない。
- `src/` は変えない。コンパイラの IR・診断・キャッシュの鍵の作り方は同じで、`TSUZURI_CLANG` がシステムの Clang のときの出力は 1 byte も変わらない。

## 仕様

1. 同梱の launcher は、`zig cc` を起動する実行のうち、次の 3 つを満たすものにだけ、`-fsanitize-trap=undefined` と `-fno-sanitize=function` をこの順で足す。
   - (a) 引数に `-c` か `-S` がある（コンパイルだけの実行）。
   - (b) `-O` で始まる引数のうち最後のものが無いか、`-O0`・`-Og`・`-O` のどれかである。
   - (c) `-fsanitize` か `-fno-sanitize` で始まる引数が 1 つもない（`-fsanitize-trap=`・`-fsanitize-recover=`・`-fno-sanitize-trap=` も含む）。
2. 判定は、`--target=` を取り除き `is_dropped` を捨てた後の引数で行う。`-x ir` の経路は、IR のコンパイルの後に残る引数で判定する（リンクの実行なので (a) に当たらない）。UTF-8 でない引数は判定に使わない（パスだけが UTF-8 でありうる）。
3. 足した結果、`-O0` 相当の C は今と同じ 17 種の検査を持ち、`__ubsan_handle_*` を参照しない。検査に当たるとメッセージを出さずにトラップ命令で終わる。x86-64 では `ud1`、Windows x64 では終了コード `0xC000001D`（Tsuzuri の `llvm.trap` と同じ終了コード）を確かめた。ほかの OS と AArch64 での終わり方は確かめていない。
4. 次の実行は今と同じ: Zig の Release の水準（`-O1` 以上）、リンクの実行（`-O0` のリンクは今どおり Zig の UBSan のランタイムを入れ、その実行の中でコンパイルする C（`-g` のない `stack.c`・`backtrace.c`、利用者の C のソース）は今どおりメッセージを出す形）、sanitizer を指定した実行、`-E`、`--version`、ARM64 Windows の native、`-x ir` の IR のコンパイル。
5. 結果として、次のものが動く。
   - `tsuzuri-clang -c`（`-O` なし・`-O0`・`-Og`・`-O`）で作った object を、`tsuzuri build` のどの `-O`、`tsuzuri-clang -O2` のリンク、`link.exe`、`lld-link`、システムの Clang でリンクできる。
   - Zig を同梱する配布物（Windows Arm64 以外の 4 環境）で、wasm32 の `-O0` の WASI・`tzroutine-single`（と、同じ経路の JSPI・threads）の build と `test --target wasm32` が通る。
   - Linux の `--emit object -O0` の object が `__ubsan_` を参照しない。
   - 配布物の `native-c-*-O0.o` も参照しない。
6. 新しいフラグ・環境変数・manifest のキーは足さない（D-55 の登録はない）。

## 設計

### `scripts/toolchain/clang.rs`

```rust
/// W37: the arguments that make Zig's Debug-mode UBSan trap, with the same checks it has by default.
pub const UBSAN_TRAP: [&str; 2] = ["-fsanitize-trap=undefined", "-fno-sanitize=function"];

/// W37: whether a `zig cc` run with these arguments compiles C with the UBSan that Zig turns on at its
/// Debug level (no `-O`, or a last `-O` that is `-O0`, `-Og` or `-O`) and the command names no sanitizer.
/// Its handlers live in Zig's UBSan runtime, which only a Debug link by Zig adds, so such an object fails
/// in a release link, in wasm-ld, and in every other linker. A link run keeps Zig's default: adding
/// UBSAN_TRAP there stops Zig from adding the runtime that the objects of a Debug link may still call.
pub fn ubsan_trap(arguments: &[OsString]) -> bool {
    let texts: Vec<&str> = arguments.iter().filter_map(|argument| argument.to_str()).collect();
    if texts
        .iter()
        .any(|text| text.starts_with("-fsanitize") || text.starts_with("-fno-sanitize"))
    {
        return false;
    }
    if !texts.iter().any(|text| *text == "-c" || *text == "-S") {
        return false;
    }
    match texts.iter().rev().find(|text| text.starts_with("-O")) {
        None => true,
        Some(level) => matches!(*level, "-O0" | "-Og" | "-O"),
    }
}
```

- 呼び出しは `main` の `zig cc` の枝の 1 か所だけにする。

```rust
let mut command = Command::new(zig);
command.args(["cc", "-target", &zig_target]);
if ubsan_trap(&arguments) {
    command.args(UBSAN_TRAP);
}
command
```

- この時点の `arguments` は、`--target=` を取り除き、`is_dropped` を捨て、`-x ir` の経路なら `-x ir` を除いて IR を `module.o` に置き換えた後のものである。`-x ir -c` の実行は、それより前に LLVM Clang だけで返る。
- ARM64 Windows の llvm-mingw の枝（`cfg!(all(windows, target_arch = "aarch64")) && !request.wasm`）には足さない。LLVM Clang は既定で UBSan を入れない（A6）。コードの上では、その環境の wasm32 は `zig cc` の枝を通る。しかし Windows Arm64 の配布物は Zig を同梱しない（`scripts/toolchain/bundle.mjs` の `platforms` に `win32-arm64` が無く、`mingwChecksums` の枝で llvm-mingw だけを置く）。そのため、その配布物で wasm32 のランタイムの C をコンパイルするビルドは、今も `cannot start the bundled toolchain` で失敗するはずである（対象外。実機では未確認）。
- `-o`（出力）と `-O`（最適化）は大文字と小文字で区別する。`-Wl,-O1` は `-W` で始まるので水準の判定に入らない。
- 足す引数は利用者の引数より前に置く。(c) により利用者の sanitizer の引数と並ぶことはないので、順序で結果は変わらない。
- `tests/targets.rs` は `#[path = "../scripts/toolchain/clang.rs"] #[allow(dead_code)] mod shim;` でこのファイルを読むので、`pub fn` と `pub const` はそのまま単体テストできる。`scripts/toolchain/bundle.mjs` は `rustc --edition=2024 -O` でこのファイルだけをコンパイルするので、crate を足さない。

### 配布物とキャッシュ

- `scripts/toolchain/runtime.mjs` は変えない。配布物の `native-c-*-O0.o` は、新しい launcher で作り直されて参照がなくなる。manifest の `objects[].args` は同じで、`tools` の launcher の SHA-256 が変わる。
- `src/runtime_cache.rs` の `prebuilt` は launcher の SHA-256 を照合し、`src/cache.rs` の `tool-binary` も変わるので、古い launcher で作った object（Zig のランタイムを参照する）が新しい launcher のビルドに混ざることはない。`src/` は変えない。

### 採らない案

- **無条件に `-fsanitize-trap=undefined` を足す**: Zig は、この引数を `-O1` 以上でも「UBSan をトラップの形で有効にする」と読む（E2）。Release の C のすべてと配布物の `-O1`〜`-O3` の object に検査が入り、性能と出力が変わる（AGENTS.md の性能の方針と、`-O1` 以上を変えない規則に反する）。
- **`-fno-sanitize=function` を付けない**: トラップの形では `function` が加わり（E1）、各関数の前に印が入る。そして、型の違う関数ポインターを通る呼び出しが新しくトラップになる。ランタイムの C は、型を変換した関数ポインターで Tsuzuri の関数や自分の関数を呼ぶので、今は無い検査でトラップする危険がある。今の 17 種にそろえる（E3）。
- **`-fno-sanitize=undefined` で検査を外す**: システムの Clang と同じになり、単純ではある。しかし、Zig の Debug の C の検査（利用者のホストの C とランタイムの C の UB を見つける手段）を黙って失う。トラップにすれば、検査は残り、終了コードは Tsuzuri のトラップと同じ種類になる。
- **リンクの実行にも足す**: Zig が UBSan のランタイムを入れなくなり、今 `-O0` のリンクで通る、Zig のランタイムを参照する object（古い launcher で作ったもの、利用者が `-fsanitize=undefined` で作ったもの）が失敗する（E5）。`-O0` の実行ファイルからランタイムを外す利点は、この破壊に見合わない（対象外）。
- **driver（`src/driver.rs`・`src/test_runner.rs`）がランタイムの C のコンパイルに足す**: コンパイラ自身のコンパイルは直る。しかし、利用者が `tsuzuri-clang -c` で作るホストの object（B1・B4・B5）は直らない。呼び出し側が 10 か所近くに分かれ、配布物の object の照合の引数（manifest の `args`）も変わる。Zig の既定の違いを吸収する場所は launcher の 1 か所である（`is_dropped`・sysroot の拒否と同じ）。
- **Zig の UBSan のランタイムを常にリンクする**: Zig のリンクの Release の水準（B1）は直る。しかし、`wasm-ld`・`link.exe`・システムの Clang（C1〜C4、B4、B5）は直らない。
- **文書に書くだけ（`-fno-sanitize=undefined` か `-O1` 以上でコンパイルせよ）**: コンパイラ自身のランタイムの C（C1〜C5）は利用者が引数を選べないので、直らない。

## 前提とする他チケットのインターフェース

- なし。

## 他チケットへ提供するインターフェース

- `scripts/toolchain/clang.rs` の `pub fn ubsan_trap(arguments: &[OsString]) -> bool` と `pub const UBSAN_TRAP: [&str; 2]`。launcher にほかの Zig の既定の補正を足すときは、同じく純粋関数と `tests/targets.rs` の表で書く。
- W01 の E2E と Windows の確認（手順 5）、W06 の `tests/` の C の補助、W03 の文書の手順は、このチケットの後、同梱の `tsuzuri-clang -c` で作った C の object を、`-fno-sanitize=undefined` を付けずに `--link`・`link.exe` へ渡せる。W01 の注記の `-fno-sanitize=undefined` は (c) により今と同じく効くので、書き換えなくても誤りにはならない。
- W03 の R4（未定義シンボルの hint）: 同梱の `-O0` の object が `__ubsan_handle_*` を未定義に残す経路がなくなる。hint の判定はこのチケットの前後で同じ。

## 実装手順

1. **変更**: なし（ブランチ `fix/w37-bundled-clang-ubsan-trap`）。
   **内容**: 着手条件。HEAD の launcher を作る（`rustc --edition=2024 -O scripts/toolchain/clang.rs -o target/scratch/w37/old/bin/tsuzuri-clang`）。`TSUZURI_ZIG`・`TSUZURI_LLVM_CLANG` を同梱の Zig と Clang に向け、`node scripts/toolchain/runtime.mjs target/scratch/w37/dist target/scratch/w37/old/bin/tsuzuri-clang` を実行して、`target/scratch/w37/dist/share/runtime/manifest.json` を `target/scratch/w37/old-manifest.json` に写す。
   **確認**: 「現状」の A1・B1・C2・C3 が再現する。写した manifest に 36 個の object がある。
2. **変更**: `tests/targets.rs` に `the_bundled_launcher_traps_zig_debug_ubsan`（テスト計画）を先に書く。
   **内容**: `shim::ubsan_trap` と `shim::UBSAN_TRAP` がまだ無いので、コンパイルが失敗することを見る。
   **確認**: `cargo test --locked --test targets the_bundled_launcher_traps` がコンパイルで失敗する。
3. **変更**: `scripts/toolchain/clang.rs`（設計）。
   **内容**: `UBSAN_TRAP`・`ubsan_trap` と、`zig cc` の枝の呼び出し。
   **確認**: 手順 2 のテストが `running 1 test` で通る。T0。新しい launcher を `target/scratch/w37/new/bin/tsuzuri-clang` に作り、手順 1 と**同じ** `target/scratch/w37/dist` に `runtime.mjs` をもう一度実行する（Zig の object はコンパイルしたディレクトリと引数を埋め込むので、場所を変えると比べられない）。新しい manifest と `old-manifest.json` の `objects` を比べ、sha256 が変わったのが `native-c-*-O0.o` だけであることを確かめる（S1）。新しい launcher を `TSUZURI_CLANG` にして、`--no-cache` で「現状」の B1・C1〜C4 を繰り返し、F1・F3 と同じ結果になることを確かめる。`-v` と新しい Zig のキャッシュで E3 の 11 通りを繰り返す（S2）。
4. **変更**: `scripts/toolchain/smoke.mjs`（テスト計画の段）。
   **内容**: `node scripts/slot.mjs -- node scripts/toolchain/bundle.mjs target/toolchain` で配布物を作り、smoke を実行する。
   **確認**: `node scripts/toolchain/smoke.mjs target/toolchain` と `--discover` が通る。HEAD の launcher の配布物では、足した段のうち S-a・S-d・S-e が失敗することを 1 度確かめる（手順 1 の launcher を `bin/` に置き換えた写しでよい）。
5. **変更**: 文書（下）。
   **確認**: `node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/platforms.md _tsuzuri/language-reference/compiler/native-interop.md`。
6. **変更**: なし。
   **内容**: T1（`cargo nextest run --locked`）、T2。PR に `ci:full` を付ける（`scripts/toolchain/**` を変えるので `targets.yml` も、smoke を回す `vscode.yml` も動く）。
   **確認**: `ci.yml`、`targets.yml`、`windows.yml`、`vscode.yml` の 5 環境が通る。`target/scratch/w37/` を消す。

## テスト計画

### 単体（`tests/targets.rs`）

`the_bundled_launcher_traps_zig_debug_ubsan`: 引数の列（`OsString` に直す）と期待値の表。

| 引数 | 期待 |
| --- | --- |
| `-c a.c` | true |
| `-O0 -c a.c`、`-Og -c a.c`、`-O -c a.c` | true |
| `-S -emit-llvm a.c`（`probe_target_attributes` と同じ形） | true |
| `-O1`・`-O2`・`-O3`・`-O4`・`-Os`・`-Oz`・`-Ofast` に `-c a.c` | false |
| `-O2 -O0 -c a.c`、`-O2 -Og -c a.c` | true（最後が勝つ） |
| `-O0 -O2 -c a.c` | false |
| `-c a.c -Wl,-O1` | true（`-Wl,` は水準ではない） |
| `-O0 a.o -o a.exe`（リンク）、`-O0 a.c -o a.exe`（コンパイルとリンク） | false |
| `-O0 -E a.c`、`--version`、`-dM -E -x c -` | false |
| `-O0 -c a.c -fsanitize=address`、`-fno-sanitize=undefined`、`-fsanitize-trap=undefined`、`-fsanitize-recover=all`、`-fno-sanitize-trap=undefined` | false |
| `-O0 -c` と UTF-8 でないパス（Unix は `OsString::from_vec`、Windows は `OsString::from_wide` の不正なサロゲート。cfg で分ける） | true |

`UBSAN_TRAP == ["-fsanitize-trap=undefined", "-fno-sanitize=function"]` も確かめる。期待値はこのチケットの A・E の測定から手で書く（コンパイラの出力から写さない）。

### smoke（`scripts/toolchain/smoke.mjs`。`vscode.yml` の 5 環境）

同梱ツールチェーンだけで回す（配布物に `llvm-nm` は無いので、参照の有無はリンクの成否で見る）。launcher を `path.join(tools, 'bin', 'tsuzuri-clang' + suffix)` とする。「Zig の環境」は Zig を同梱する 4 環境（`!(process.platform === 'win32' && process.arch === 'arm64')`。Windows Arm64 の配布物は Zig を持たない）とする。

- **S-a**（全環境）: `int tz_ub_add(int a, int b) { return a + b; }` の `ub.c` を `-c ub.c -o ub.o`（`-O` なし）でコンパイルする。`main` で `tz_ub_add(argc > 1 ? 2147483647 : 40, argc > 1 ? 1 : 2)` を `printf` する `ub-main.c` と `-O2` でリンクする（今は Zig の環境で `__ubsan_handle_add_overflow` が未定義）。引数なしで `42\n`。
- **S-b**（Zig の環境）: S-a の実行ファイルに引数を 1 つ渡すと、標準出力が空のまま、Windows は終了コード `0xC000001D`（`3221225501`）、POSIX はシグナルで終わる（x86-64 は `SIGILL`、AArch64 は `SIGTRAP` のはずだが確かめていないので、検査は `result.signal !== null` だけにする）。smoke の `run` は終了コードしか見ないので、`spawnSync` を直接使う。`assert` で呼び出しを包まない（`ci.instructions.md` の `NDEBUG` の教訓）。
- **S-c**（Zig の環境）: `-O2 -c ub.c -o a.o` を launcher で、`-O2 -c ub.c -o b.o` を `zig/zig cc -target <Zig の target>` で、同じディレクトリでコンパイルし、2 つが 1 byte も違わない（launcher が `-O2` に何も足さないこと）。Zig の target は `resolve(None, ..)` と同じ `<x86_64|aarch64>-<windows-gnu|linux-musl|macos.12.0>`。
- **S-d**（Zig の環境）: `do! IO.writeln "hi"` の `main` を `build --target wasm32 --wasm-host wasi -O0 --no-cache` にし、`node:wasi`（`version: 'preview1'`）で実行して `hi\n` と終了コード 0。
- **S-e**（Zig の環境）: 「現状」C3 の `Channel` の test を `test --target wasm32 --json`（既定の `-O0`）で実行し、`"passed":1`。
- **S-f**（Linux だけ）: IO を使う `export def greet :: i64 -> i64` を `build --emit object -O0 --no-cache` にし、`tz_greet(1)` を呼ぶ C と、launcher の `-O2` でリンクして実行する（今は `__ubsan_handle_*` が未定義）。Linux のホストで実際に通すまで、期待する標準出力（`hi`）と終了コードは実装者が確かめて書く。

### 手元（Windows、手順 3・4）

- 配布物の `share/runtime` の sha256 の比較（S1）。
- 新しい launcher の `-c host.c` の object を、Developer PowerShell の `link.exe /nologo /dll /noentry host.o /out:h.dll` と、`TSUZURI_CLANG` を LLVM の `clang.exe` にした `build app --link host.o` でリンクできる（B4・B5 の逆。起票時に試作で確かめた F6 を、実装した launcher でもう一度行う）。
- 速度の合否は入れない（性能は変えない。`-O0` の実行ファイルの大きさも、リンクの実行には足さないので変わらない）。

## ドキュメント

- `_tsuzuri/language-reference/compiler/platforms.md` の「実行ファイルをリンクする」の「同梱ツールチェーン」の箇条に 1 項目足す（です・ます。案）:「C を `-O` なし・`-O0`・`-Og`・`-O` でコンパイルするとき、Zig は未定義動作の検査（UBSan）を入れます。同梱の `tsuzuri-clang` は、`-c`・`-S` のコンパイルで `-fsanitize` 系の指定がなければ、その検査を、当たるとトラップ命令で止まる形にします（`-fsanitize-trap=undefined -fno-sanitize=function`）。そのため object は Zig の UBSan のランタイムを参照せず、ほかのリンカーや `-O1` 以上のリンクでも使えます。検査に当たるとメッセージは出ません（Windows x64 では終了コード `0xC000001D`）。`-O1` 以上のコンパイルは検査を入れません。1 回の実行でコンパイルとリンクをするときは Zig の既定のままで、`-O0` のリンクは Zig の UBSan のランタイムを入れ、検査に当たるとメッセージを出して終わります」。「確かめた範囲」の `vscode.yml` の行に、smoke が wasm32 の `-O0` の WASI・`Channel` を確かめることを足す。
- `_tsuzuri/language-reference/compiler/native-interop.md` の「リンク入力」: 「同梱の `tsuzuri-clang -c` で作ったホストの object は、`tsuzuri build` のどの `-O` でも、ほかの Clang や MSVC のリンカーでもリンクできます」を 1 文足し、`platforms.md` の「実行ファイルをリンクする」へのリンク（`[対応プラットフォーム](platforms.md#実行ファイルをリンクする)`。`native-interop.md` からの相対パス）を付ける。
- `docs/architecture.md` の 447 行目（同梱ツールチェーンの `scripts/toolchain/clang.rs` の段落）に、`ubsan_trap` の条件と理由（Zig の Debug の UBSan のランタイムは Debug のリンクでしか入らない。リンクの実行と `-O1` 以上には足さない。`function` を外して既定の 17 種にそろえる）と、`tests/targets.rs`・smoke の検証を書く。PB01 の配布物の object の段落（324 行目付近）は、`-O0` の `native-c` がトラップの形であることを 1 文足すか確かめる。
- `docs/language.md`・README は変えない（言語の意味、CLI、検証のコマンドは変わらない）。利用者向けの文書から W チケットへリンクしない（W03 D6 と同じ理由）。

## 受け入れ条件

- [ ] `tsuzuri-clang -c`（`-O` なし・`-O0`・`-Og`・`-O`）の object が `__ubsan_` を参照しない。その object を、`tsuzuri build`（既定の `-O3`）の `--link`、`tsuzuri-clang -O2` の object だけのリンク、Windows の `link.exe`・MSVC の `lld-link` でリンクできる。
- [ ] Zig を同梱する 4 環境の同梱ツールチェーンで、wasm32 の `-O0` の `--wasm-host wasi` の build と実行、`Channel` を使い起動しないプログラムの build、`test --target wasm32`（既定の `-O0`）が通る。
- [ ] `-O1` 以上のコンパイルの出力が 1 byte も変わらない（同じ場所で作った `share/runtime` の比較で、変わるのは `native-c-*-O0.o` と manifest の `tools` だけ。smoke の S-c）。
- [ ] `-O0`・`-Og` のコンパイルの検査の種類が、`NATIVE_TARGETS` の 9 個・wasm32・ホストの既定のすべてで今の 17 種と同じ（`-v` の cc1）。
- [ ] リンクの実行、sanitizer を指定した実行、`-E`、`--version`、`-x ir` の経路、ARM64 Windows の llvm-mingw の枝の引数が変わらない（単体テスト。B2・B3・D1 の形が今と同じに動く）。
- [ ] `src/` を変えていない。`TSUZURI_CLANG` がシステムの Clang のときの IR・object・診断は変わらない。
- [ ] `the_bundled_launcher_traps_zig_debug_ubsan` と、smoke の S-a〜S-e（Linux は S-f も）が通る。
- [ ] 文書を直し、`check-docs` が通る。
- [ ] T2 が通る。PR に `ci:full` を付け、`vscode.yml` の 5 環境と `windows.yml`・`targets.yml` が通る。

## 落とし穴

- **Zig の `-fsanitize-trap=undefined` は Clang と意味が違う**: Clang では、有効な sanitizer の反応を変えるだけである（A6）。Zig では、`undefined` の群をトラップの形で有効にする（E2）。水準で絞らずに足すと、`-O2` の C に検査が入る。
- **`function`**: トラップの形の既定に加わり、関数の前に印（`0xc105cafe` と型のハッシュ）を置く。型の違う関数ポインターを通る呼び出しもトラップになる。外し忘れると、今は無いトラップがランタイムの C に現れうる。
- **リンクの実行に足さない**: 足すと、Zig は Debug のリンクでも UBSan のランタイムを入れなくなる（E5）。「コンパイルだけ」の判定は `-c`・`-S` で行う。`-E` と `-M` 系はコンパイルしないので、対象にしない。
- **引数のない `-O`**: Clang では `-O1` だが、Zig 0.16.0 では Debug（A3）。Clang の意味で判定しない。Zig の版を上げたら、A1〜A5 と E3 を確かめ直す（S2）。単体テストが固定するのは launcher の規則で、Zig の振る舞いは smoke（S-a・S-c）が見る。
- **Zig のキャッシュ**: `zig cc -v` は、Zig 自身のキャッシュに当たると cc1 の行を出さない。種類を確かめるときは、毎回新しい `ZIG_GLOBAL_CACHE_DIR`・`ZIG_LOCAL_CACHE_DIR` にする（smoke はすでに一時ディレクトリに向けている）。
- **バイト比較の条件**: Zig の object は、コンパイルしたディレクトリ（`-fdebug-compilation-dir`）と cc1 の引数（CodeView の build info と、Zig の一時ファイルの名前のハッシュ）を埋め込む。比べるのは、同じディレクトリで同じ引数を渡したものどうしに限る（出力のファイル名は埋め込まれない。F2 で確かめた）。
- **トラップの終わり方は OS と CPU で違う**: S-b は「シグナルで終わる、または `0xC000001D`」だけを見る。AArch64 の終わり方を文書で言い切らない。
- **ARM64 Windows**: native は llvm-mingw の LLVM Clang で UBSan が無い。配布物は Zig を同梱しない（`bundle.mjs` の `platforms` に無い）ので、wasm32 のランタイムの C もコンパイルできない。そのため S-b〜S-e は省き、S-a だけを回す（UBSan が無いので、今も通るはずの段）。ARM64 Windows の実機では確かめていない（起票の時点では x64 の機械だけだった）。

## 対象外

- Zig のリンクの実行の既定（`-O0` のリンクが Zig の UBSan のランタイムを入れること、1 回の実行でコンパイルとリンクをする C がメッセージを出して終了コード 3 で終わること）。`-O0` の実行ファイルから Zig の UBSan のランタイムを外すこと（E5 の破壊を避ける別の設計が要る）。
- `-O1` 以上の C に検査を入れること、システムの Clang の既定を変えること。
- `-x ir` の経路が `-W` で始まる引数をすべて LLVM Clang に渡すので、Windows の同梱ツールチェーンのビルドで `clang: warning: -Wl,--gc-sections: 'linker' input unused [-Wunused-command-line-argument]` が出ること（`clang.rs` の `-x ir` の枝の `text.starts_with("-W")`。`src/driver.rs` の `dead_strip_args` が同梱の launcher に `-Wl,--gc-sections` を渡す。B1・C6 のどの実行にも出た。別の小さな修正: [W40](W40-launcher-ir-forwards-linker-flags.md)。同じ `clang.rs` と `tests/targets.rs` を触るが行は重ならず、どちらが先に入ってもよい（W40 の「W37 との関係」））。
- Zig が `-O1` から `NDEBUG` を定義すること（`ci.instructions.md` に記録済み）。
- C のソースをリンク入力として正式に受けること（W03 D3、E12 D6）。
- `--emit bitcode` のランタイム入りの経路を、`llvm-link` のある Linux・macOS で確かめること（C7。この修正で `-S` の実行にも足すので直るはずだが、確かめるのは実装者の手元か `targets.yml`）。
- Windows Arm64 の配布物が Zig を同梱しないため、そこで wasm32 のランタイムの C（WASI・`Channel`・JSPI・threads）をコンパイルできないこと（launcher の `zig cc` の枝が存在しない `zig/zig.exe` を起動する。コードと `bundle.mjs` から読んだ既存の制限で、実機では未確認。別のチケット）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 直す場所は launcher（`scripts/toolchain/clang.rs`）の `zig cc` の枝の 1 か所 | 利用者の `tsuzuri-clang -c` とコンパイラ自身のコンパイルの両方に効く。driver の呼び出し側（10 か所近く）と配布物の照合の引数を変えずに済む。Zig の既定の違いを吸収するのは launcher の役目である（`is_dropped`・sysroot の拒否） | 既定案（実装者はこの案に従う） |
| D2 | 検査は外さず、トラップにする（`-fsanitize-trap=undefined`） | Zig の Debug の C の UB の検査を保ち、検査に当たったときの終了コードを Tsuzuri のトラップと同じ種類にする。`-fno-sanitize=undefined` は検査を黙って失い、文書だけでは C1〜C5 が直らない | 既定案（実装者はこの案に従う） |
| D3 | `-fno-sanitize=function` を添え、検査の種類を今の 17 種にそろえる | トラップの形で加わる `function` は、型を変換した関数ポインターの呼び出しを新しくトラップにし、ランタイムの C で今は無い停止を生みうる。反応の変更だけに範囲を絞る（E1・E3） | 既定案（実装者はこの案に従う） |
| D4 | Zig が Debug の水準のとき（`-O` なし、または最後の `-O` が `-O0`・`-Og`・`-O`）だけ足す | Zig はこの引数を `-O1` 以上でも検査を有効にすると読み（E2）、Release の出力と性能を変える。水準の写像は Zig 0.16.0 の実測（A1〜A3）で、Clang の `-O` の意味ではない | 既定案（実装者はこの案に従う） |
| D5 | コンパイルだけの実行（`-c`・`-S`）に限り、リンクの実行には足さない | リンクの実行に足すと Zig が UBSan のランタイムを入れなくなり、今 `-O0` でリンクできる組み合わせを壊す（E5）。`-S` は `--emit bitcode`・macOS の `-g` の object・`probe_target_attributes` が使う | 既定案（実装者はこの案に従う） |
| D6 | 利用者が `-fsanitize*`・`-fno-sanitize*` を 1 つでも渡したら何も足さない | 利用者の選択（W01 の注記の `-fno-sanitize=undefined`、ASan・UBSan のビルド）を上書きしない | 既定案（実装者はこの案に従う） |
| D7 | 新しいフラグ・環境変数で切り替えられるようにしない | 必要な利用者は D6 のとおり自分で sanitizer を指定できる。D-55 の登録を増やさない | 既定案（実装者はこの案に従う） |
