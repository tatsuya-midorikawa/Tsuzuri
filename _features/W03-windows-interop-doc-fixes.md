# W03: Windows のネイティブ連携の文書と診断の誤りを直す

| 項目 | 内容 |
| --- | --- |
| ID | W03 |
| 優先度 | P2（言語の意味は変えない。誤った文書のとおりに書くと `E1008` で止まるか、`tsuzuri_free` を export しない DLL を作ってしまう。リンクの失敗の hint は原因と無関係な導入の案内を出す） |
| 規模 | S |
| 依存 | – |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 不要（文書と診断の文だけを直す。言語の意味、生成コード、診断のコードと kind は変えない） |
| 改善する劣位 | 文書の誤り: extern の引数型、COFF の object の制限の主語、DLL の回避策の欠落、リンク入力を受けるコマンド、`[native] link` の受ける入力。診断の誤り: 未定義シンボルによるリンクの失敗に「Clang を入れよ」と出る。記載の欠落: 標準ハンドルのないプロセスの `IO` |
| 手本にする既存実装 | `src/driver.rs` の `link_shared` の hint（`unresolved extern symbols need --link, -l or -L`）と `tool_output`、`tests/ffi_extensions.mjs` の `rejected`、`tests/windows.mjs` の `diagnostic` |
| 主な影響ファイル | `docs/language.md`、`docs/architecture.md`、`_tsuzuri/language-reference/compiler/{platforms,native-interop,option}.md`、`_tsuzuri/language-reference/organizing-tsuzuri/packages.md`、`_tsuzuri/language-reference/built-in-types-and-modules/io.md`、`src/driver.rs`（`BuildOptions::validate_shared` の文、`tool_output`、テスト）、`src/backend/target.rs`（comment だけ）、`tests/e2e.mjs`、`tests/windows.mjs`、`tests/ffi_extensions.mjs` |

## 実装者向け早見

- 範囲: 「現状」の表の R1〜R7 を直す。コードの変更は、`BuildOptions::validate_shared` の Windows の文、`tool_output` が選ぶ hint、`src/backend/target.rs` の comment の 3 つだけ。
- 最初に読む: ① 「現状」の表 ② 仕様の R3・R4 ③ `src/driver.rs` の `tool_output` と `link_shared`。
- 最初に書くテスト: `src/driver.rs` の単体テスト `undefined_symbol_hints_name_the_symbols`（このチケットに写したリンカーの実際の出力を入力にする）。
- 触ってはいけない: `link inputs require a native executable; ...` の文（E13 Phase 2 の判断 8 が意図して残した）、`cannot execute '<tool>': ...` の hint（G10 の診断の表が固定する）、`E1008` の文、docs/language.md の「コールバック」節の寿命の 2 文（W12 の担当）、`--backend fast` の判定（R6）。
- 完了の見分け方: Windows で `extern def host_value :: unit -> i64` を呼ぶプログラムの `tsuzuri build` が `E2002` になり、メッセージに `unresolved symbols: tsuzuri_host_Main_host_value` があり、`install LLVM/Clang 17+` がない。

## 目的

Windows でネイティブ連携を試すと、文書のとおりに書いても動かない所が続けて出る。`string` を値で取る `extern` は `E1008`、object をつないだ DLL は `tsuzuri_free` を export せず、ホストのライブラリを忘れたリンクの失敗は「Clang を入れよ」と案内する。言語と生成コードは変えずに、文書を今の実装に合わせ、リンクの失敗の hint を原因に合わせる。

関連: [GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30)、[G10](_completed/G10-windows.md)（D8・D12、後続の作業）、[E12](_completed/E12-ffi-extensions.md)（D6）、[E13](_completed/E13-host-bindings.md)（Phase 2 の判断 8・11）、[PB05](../_perfs/_completed/PB05-fast-debug-backend.md)（WP14）、[W05](W05-windows-shared-library.md)。

## 着手条件と停止条件

### 着手条件

- `node scripts/repo-kb.mjs brief W03` を読む。「現状」の再現（R1・R4・R5・R7 のコマンド）を HEAD でもう一度実行し、結果が同じであることを確かめる。
- W05 が先に `main` に入っていたら、R3 の段（E2000 の文と回避策）は行わない（W05 がゲートを外し、同じ行を書き換える）。W05 が後なら、W05 の側が R3 の文を置き換える。
- [W39](W39-windows-multiple-objects-weak-defaults.md)（Windows の弱い定義を `comdat any` にする）が先に入っていたら、R3 の回避策の「link.exe は弱い定義を export できない」「import library を `.def` から作り直す」は成り立たなくなる（W39 の後は link.exe の `/DEF` でも export でき、リンカーが作る import library の名前も正しい）。着手時に W39 の状態を確かめ、回避策の文をその時点の振る舞いに合わせる。

### 停止条件

- **S1** R1〜R7 のどれかで、文書ではなく実装の方が意図に反すると分かった（たとえば `string` を値で受けるのが仕様の意図だった記録が見つかった）。文書を合わせずに報告する。
- **S2** R4 の分類が、未定義シンボル以外の失敗（Clang の IR の誤り、ライブラリのファイルが無い）を未定義シンボルと読む例が、テストの入力で 1 つでも出た。
- **S3** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

再現は Git Bash で `T=/c/Users/tmidorikawa/.vscode/extensions/tmidorikawa.tsuzuri-0.1.6-win32-x64/toolchain` とし、`TSUZURI_CLANG=$T/bin/tsuzuri-clang.exe target/debug/tsuzuri.exe ...`（MinGW）、MSVC は Developer PowerShell と `TSUZURI_CLANG="C:\Program Files\LLVM\bin\clang.exe"` で行った。

| # | 場所 | 今の文 | 実際（確かめ方） |
| --- | --- | --- | --- |
| R1 | `docs/language.md:453`（「ホスト関数のインポート」） | 引数に「共有バッファ／文字列（`ref [T]`、`string`、`utf8string`）」 | `src/abi.rs` の `parameter` は `Type::Reference(inner, false)` のバッファだけを受ける。`extern "f" def f :: string -> i32`・`:: utf8string -> i32`・`:: [i64] -> i32` はどれも `check` が `E1008`（`extern signatures support scalar/unit, shared ABI buffers and scalar records, ...`）、`:: ref string -> i32`・`:: ref utf8string -> i32` は通る |
| R2 | `compiler/platforms.md:166`（既知の制限） | 「同梱ツールチェーンは、Mach-O の `--emit object` で C ランタイムを含むものを作れません。Windows の COFF のランタイム埋め込み object も `E2002` です」 | 後半は同梱ツールチェーンの制限と読めるが、`src/driver.rs` の `build_complete`（4373 行）の判定は `platform.windows() && native_runtime && options.emit == Emit::Object` でツールチェーンを見ない。`tests/windows.mjs` は MSVC の CI で同じ `E2002` を確かめている（`rejected` の行）。`--trap-mode return` の object でも同梱の MinGW で `E2002`（`Windows COFF objects with embedded tzroutine, CPU, IO, or Net runtime are not supported; ...`）を再現した |
| R3 | `src/driver.rs` の `BuildOptions::validate_shared` の文、`compiler/native-interop.md:277`、`compiler/option.md:517`、`docs/language.md:4965` | 「`--emit object` をホストの DLL にリンクしてください」（E2000 の文は `build an object with --emit object and link it into a DLL`） | 下の「R3 の実験」。`tsuzuri_alloc`・`tsuzuri_free` は `src/llvm_abi.rs` の `allocator`・`native_allocator` が `define weak` で出し、`llvm::windows_abi` は `tz_*` と `tsuzuri_main` にしか `dllexport` を付けないので、そのままの DLL は 2 つを export しない。所有結果を返すライブラリは使えない |
| R4 | `src/driver.rs` の `build_complete`（5274〜5277 行、`linker_hint(cross_hint(platform, "install LLVM/Clang 17+ or set TSUZURI_CLANG to its executable"))`）、同 4711〜4714 行（codegen unit）、`src/test_runner.rs` の `compile_native_runner`（`runner.tools_hint` = `tests require LLVM/Clang 17+ or TSUZURI_CLANG` など） | リンクが失敗すると、原因を問わずこの hint を `tool_output` が末尾に足す | `@trusted extern "GdiFlush" def gdi_flush :: unit -> i32` を呼ぶプログラムの `build` は `lld-link: error: undefined symbol: GdiFlush` の後に `install LLVM/Clang 17+ or set TSUZURI_CLANG to its executable`（`E2002`、kind `external_tool_failed`）。名前のない `extern def host_value` でも `undefined symbol: tsuzuri_host_Main_host_value` の後に同じ hint。`link_shared` だけは `unresolved extern symbols need --link, -l or -L` を出す。この機械で実際の出力を見たのは lld-link（同梱の MinGW）と link.exe（`LNK2001`・`LNK2019`）だけで、ld.lld（ELF）・GNU ld・ld64 の出力は未確認 |
| R5 | `built-in-types-and-modules/io.md`、`compiler/platforms.md` | 標準ハンドルのないプロセスについて書いていない | `do! IO.writeln "hello"` の exe の PE の Subsystem を 2 に書き換え（Node で `PE\0\0` の後 24 + 68 バイト目の `u16`）、PowerShell の `Start-Process -Wait -PassThru` で起動すると、同梱の MinGW でも MSVC でも終了コード `0xC000001D`。`IO.try_writeln` は `Error IO.WriteFailed`、`IO.try_readln ()` は `Error IO.ReadFailed` を返した（どちらも MinGW と MSVC で確認。書き換えない console の exe は 7 で終わる）。標準エラーも無いので、トラップの位置の報告も出ないはずである（推測。窓のない起動では出力を見る手段がなく、確かめていない） |
| R6 | `compiler/platforms.md:169`、`compiler/option.md:270`・`275`・`299` | `--backend fast` は Windows で `E2000` | 文書が正しい。`BuildOptions::validate_fast` は `cfg!(all(any(target_os = "macos", target_os = "linux"), ...))` でないホストを `fast_backend_unavailable`（E2000）にする。このホストで `--backend fast`、`--backend fast --target wasm32` はどちらも `--backend fast is not available on windows-x86_64; use --backend llvm`、`--target x86_64-pc-windows-msvc` は `supports only the native target and wasm32`。`src/backend/target.rs` の `Target::new` の x86-64 COFF は、`tests/fast_backend.rs` の `objects_for_other_machines_have_their_formats` が `backend::compile` を直接呼ぶときだけ届く（PB05 の WP14「Windows は、ホストでは E2000 のまま」）。`isa` は `unwind_info=false` |
| R7 | `compiler/native-interop.md:230`、`compiler/option.md:15`、`docs/language.md` の「リンク入力」（495 行の節の 497・500 行）、`organizing-tsuzuri/packages.md:308〜319` | native-interop.md「`build` の `--emit exe`、`run`、native の `test` だけが受けます」、option.md:15「`build`、`run`、`test` と `--emit shared` だけ」、language.md「`build`、`run`、`test` コマンドでのみ有効」「実行ファイル以外の `--emit` は `E2000`」、packages.md「`link`: リンクする C ソースやオブジェクト」（例 `link = ["src/helper.c"]`） | `src/main.rs` の `links_apply` は `run`、native の `build` の `exe`・`shared`、native の `test`・`bench` に許す。`bench Main.tz --link nothere.o` と `script --link nothere.o Main.tz` は `E2001`（読めない入力。受理された）、`build --emit object --link ...` は `E2000`。option.md の表（59 行）の `bench`・`script` の ○ は正しい。`LinkInputs::add_to` はパスを `-x none` の後に渡すので、`--link helper.c` は Clang が拡張子でコンパイルして通る（同梱ツールチェーンで確認）が、それを確かめるテストはない（E12 D6 の契約は object と静的ライブラリ） |

### R3 の実験（`export def make_tag :: i64 -> [ubyte]` と `export def add` のライブラリ）

| 手順 | 結果 |
| --- | --- |
| `--emit object` の `llvm-nm` | `W tsuzuri_alloc`・`W tsuzuri_free`、実体は `.weak.tsuzuri_alloc.default.tz_make_tag` |
| `link.exe /DLL`（export の指定なし） | export は `tz_add`・`tz_make_tag` だけ |
| `link.exe /DLL /EXPORT:tsuzuri_free ...`、`/DEF:tag.def` | どちらも `LNK2001: unresolved external symbol tsuzuri_alloc`（link.exe は弱い外部シンボルを export できない） |
| `lld-link /DLL /DEF:tag.def`、同梱の `tsuzuri-clang -shared tag.o tag.def` | DLL の export 表は 4 つとも正しいが、リンカーが書く import library の名前は `__imp_.weak.tsuzuri_free.default.tz_make_tag`。その import library でリンクした C のホストは `undefined symbol: tsuzuri_free` |
| 同じ `.def` から `lib /DEF:tag.def /MACHINE:X64`、`llvm-dlltool -m i386:x86-64 -d tag.def -l tag.lib`、同梱の `zig dlltool`（`toolchain/zig/zig.exe`、0.16.0） | どれも `__imp_tsuzuri_alloc`・`__imp_tsuzuri_free`・`__imp_tz_add`・`__imp_tz_make_tag`、DLL 名は `.def` の `LIBRARY` |
| lld-link の DLL と `lib /def` の import library で、ネイティブ連携のページの C のホスト（`tz_make_tag(&tag, 3)` と `tsuzuri_free`） | `ABC`、終了コード 0（MSVC） |

ランタイムを要するプログラムの `--emit object` は R2 の `E2002` なので、この手順が使えるのはランタイムを要しないライブラリだけである。同梱の MinGW の経路は、DLL の export 表と `zig dlltool` の import library の名前までを確かめ、C のホストをその DLL で動かすことは確かめていない（未確認。手順 5 で確かめるか、文書に「MSVC で確認」と書く）。

## 仕様

文書のページは日本語の「です・ます」で書く（GUIDE-LITE §8）。下の文は案で、意味を変えずに言い回しを整えてよい。

- **R1**: `docs/language.md:453` の引数の列挙を「共有借用のバッファ／文字列（`ref [T]`、`ref string`、`ref utf8string`。`ref` のない `[T]`・`string`・`utf8string` の引数は `E1008` です）」にする。戻り値の列挙（所有権付きバッファ／文字列）は正しいので変えない。
- **R2**: `platforms.md:166` を 2 つの項目に分ける。Mach-O の文は変えない（Windows のホストでは確かめていない）。新しい項目:「Windows の COFF の `--emit object` は、tzroutine・CPU・IO・Net のランタイムを含むプログラムでは、ツールチェーン（MSVC、同梱の MinGW）を問わず `E2002` です。`--emit llvm` で出してランタイムを 1 度だけリンクするか、実行ファイルを作ってください。ランタイムを要しない object と LLVM IR は作れます」。
- **R3**: E2000 の文を `'--emit shared' is not supported on Windows yet (G10); build an object with --emit object, link it into a DLL with lld-link or MinGW and a .def file that exports tsuzuri_alloc and tsuzuri_free, and make the import library from the .def` にする（kind `option_unsupported_platform` と `(G10)` の印は変えない。G10 D12）。`native-interop.md` の「共有ライブラリと各言語のバインディング」の Windows の文を、次の手順の小節（見出し案「Windows の DLL（`--emit shared` が使えるまで）」）に置き換える: (1) ランタイムを要しないライブラリを `--emit object` にする（要するものは R2 の `E2002`）。(2) `--emit header` のプロトタイプの `tz_*` と `tsuzuri_*` を `EXPORTS` に並べ、`LIBRARY "<DLL のファイル名>"` を書いた `.def` を作る。(3) `lld-link /DLL <obj> libcmt.lib /DEF:<def> /OUT:<dll>`（Developer PowerShell）か、同梱の `tsuzuri-clang -shared <obj> <def> -o <dll>` でリンクする。`link.exe` は弱い定義の `tsuzuri_alloc`・`tsuzuri_free` を export できない（`LNK2001`）。(4) リンカーが書く import library は名前が `.weak.` で始まり使えないので、同じ `.def` から `lib /DEF:<def> /MACHINE:X64 /OUT:<lib>`、`llvm-dlltool -m i386:x86-64 -d <def> -l <lib>`、または同梱の `zig dlltool` で作り直す（Arm64 は `/MACHINE:ARM64`・`-m arm64`）。(5) C#・Python のバインディングは名前で読むので (3) の DLL だけで足り、C・C++ のホストは (4) の import library でリンクする。`option.md:517` の行と `docs/language.md:4965` の「Windows（DLL の出力は未対応です…）」は、新しい E2000 の文と、この小節へのリンクに合わせる。
- **R4**: リンクの失敗の出力が未定義シンボルを示すときは、hint を次の文に置き換える（ツールの出力はそのまま残す）: `unresolved symbols: <名前>, ...; link the objects or libraries that define them with --link PATH, -l NAME and -L DIR, or list them under [native] in Tsuzuri.toml (an 'extern def' without a link name is the host function tsuzuri_host_<Module>_<name>)`。名前は出てきた順に重複を除いて 8 個まで、残りは `and N more`。コードは `E2002`、kind は `external_tool_failed` のまま。ツールを起動できない（`cannot execute`）とき、未定義シンボル以外で失敗したときの hint は今と同じ。`TSUZURI_LINKER=lld` の注記（`linker_hint`）と `cross_hint` の前置きは、未定義シンボルの場合には付けない（原因と無関係なので）。wasm-ld の失敗には適用しない（WebAssembly のビルドはリンク入力を `E2000` にするので、`--link` の案内は誤りになる）。
- **R5**: `io.md` の「標準出力、標準エラー、標準入力」の後に、次の段落を足す:「標準ハンドルを持たずに起動されたプロセス（Windows の GUI サブシステムの実行ファイルを Explorer や `Start-Process` から起動したときなど）では、書き込みも読み込みも失敗します。`try_` の付かない関数はトラップします（Windows では終了コード `0xC000001D`）。標準エラーも無いので、トラップの位置の報告は読めません。`try_writeln` などは `IO.WriteFailed`、`try_readln` は `IO.ReadFailed` を返します」（報告が読めないのは推測なので、「出ません」と言い切らず「読めません」と書く）。`platforms.md` の「既知の制限」に 1 行を足し、io.md の段落へリンクする。POSIX の閉じた fd については確かめていないので書かない。
- **R6**: 判定は変えない（D1）。`platforms.md:169` の「Windows」を「Windows のホスト（wasm32 も）」にし、`option.md:270` の target の欄に「どちらもビルド機が macOS Arm64・Linux x64/Arm64 のときだけです」を足す。`docs/architecture.md` の「高速デバッグバックエンド（PB05）」に「翻訳器は x86-64 の COFF も書けるが（PB05 の WP14 のテストだけが使う）、CLI は Windows のホストと `--target TRIPLE` を `E2000` にする。有効にするには `unwind_info` を true にして `.pdata` を書くことと、`windows_abi` の 128 bit の helper の規約を Windows で確かめることが要る」を足す（x86-64 の helper の規約の食い違いは [W42](W42-windows-i128-division.md) が直す。足す文には `_features` へのリンクを入れない）。`src/backend/target.rs` の `Target::new` の COFF の分岐の上に、CLI から届かないことと上の 2 つの条件を 2 行の comment で書く。
- **R7**: リンク入力を受けるのは、native の `build`（`--emit exe` と `--emit shared`）、`run`、native の `test` と `bench`、`script`（ファイルより前に書いたもの）で、`test --doc` は受けない。`native-interop.md:230`、`option.md:15`、`docs/language.md` の「リンク入力」の 2 文をこれに合わせる（language.md の「実行ファイル以外の `--emit`」は「実行ファイルと共有ライブラリ以外の `--emit`」にする）。受ける入力は object と静的ライブラリ（D3）。`packages.md` の例を `link = ["host.o", "vendor/libutil.a"]` に、`link` の説明を「リンクするオブジェクトか静的ライブラリの相対パス（`--link` と同じ）」にし、`option.md:433` と `packages.md` に「ほかの種類のファイルもそのまま Clang に渡すので、Clang が拡張子で扱いを決めます（C のソースはその場でコンパイルされますが、オプションは決まっておらず、検証していません）。オブジェクトにしてから渡してください」を足す。

## 設計

### R4: 未定義シンボルの分類（`src/driver.rs`）

```rust
/// W03: the symbols that a failed link reports as undefined, in the order the linker printed them,
/// each once. Empty when the output shows another failure.
fn undefined_symbols(output: &str) -> Vec<String>;
```

- 読む形は 4 つ。lld（ELF・COFF・MinGW）の `undefined symbol: NAME`、link.exe の `unresolved external symbol NAME`（`LNK2001`・`LNK2019`。`LNK1120` の集計の行は読まない）、GNU ld の ``undefined reference to `NAME'``、ld64 の `Undefined symbols for architecture ARCH:` に続く `"_NAME", referenced from:`（先頭の `_` を外す）。lld-link と link.exe はこの機械の実際の出力で確かめた。ld.lld（ELF）・GNU ld・ld64 は確かめていない（テストの入力は各ツールの文書どおりに書き、手順 4 の `tests/e2e.mjs` の段で Linux の CI の実際の出力（Ubuntu の Clang の既定のリンカー）と照らす。macOS は手元か `targets.yml` の結果で照らす）。
- `wasm-ld:` で始まる行は読まない。`tool_output` も、プログラム（`name` のファイル名の stem）が `wasm-ld` のときは置き換えない（仕様 R4）。
- 名前の取り出し: 空白までを取り、前後の引用符と行末の `\r` を外す。lld が `__declspec(dllimport) NAME` と書く形（W17 の `dllimport` の後に出る）はその前置きを外す。
- `tool_output` の失敗の分岐で `undefined_symbols(&text)` が空でなければ hint を置き換える。`tool_output` はビルド・`test`・`bench` の runner（`src/test_runner.rs` は `driver` の子 module で同じ `run_tool` を使う）・`link_shared` が共有するので、1 か所で全部に効く（D4）。

### R3・R6 のコード

- `BuildOptions::validate_shared` の文字列と、それを固定する `validates_shared_libraries_and_native_bindings` の `cfg!(windows)` の分岐の期待値（`src/driver.rs` 7630 行付近）を同じ文に直す。
- `src/backend/target.rs` は comment だけ。コードは変えない。

## 前提とする他チケットのインターフェース

- なし。W05（DLL の出力）と同じ行（`validate_shared` の文、native-interop.md の Windows の文、option.md:517、docs/language.md:4965）を書き換えるので、後に入る方が rebase で前の文を置き換える。

## 他チケットへ提供するインターフェース

- `undefined_symbols`: W17（`@link`）がシステムライブラリを足し忘れたときの案内、W27（生成した `win32` パッケージ）の利用者の案内がこの hint を使う。
- io.md の「標準ハンドルのないプロセス」の段落: W08 が意味を決めたら置き換える。
- platforms.md の R2 の文（ランタイム入りの Windows の COFF object の汎用の `E2002`）: [W11](W11-windows-os-api.md) が `os_apis` を Windows で真にすると、同じ文に OS のランタイムが加わる。W03 と W11 のうち後に入るほうが、文書の列挙と `src/driver.rs` の文を合わせる。

## 実装手順

### 手順 1: ベースライン

- 変更: なし（再現は `target/scratch/w03/` の下だけ）。
- 内容: 「現状」の表の再現コマンドを実行し、結果を控える。`grep -n "install LLVM/Clang 17\|tests require LLVM\|benchmarks require LLVM" src/*.rs` で hint の出所を控える。
- 確認: 表と同じ結果。違えば停止条件 S1。

### 手順 2: R4 の分類

- 変更: `src/driver.rs`（`undefined_symbols`、`tool_output`、単体テスト）。
- 内容: 「設計」のとおり。先に単体テストを書き（「テスト計画」の入力）、失敗を見てから実装する。hint の文は仕様 R4 のとおり。
- 確認: `cargo test --locked --lib undefined_symbol`（`running N tests` の N が足したテストの数）。T0。

### 手順 3: R3 の文

- 変更: `src/driver.rs`（`validate_shared` の文と `validates_shared_libraries_and_native_bindings`）。W05 が先に入っていれば飛ばす。
- 内容: 仕様 R3 の英文に置き換え、テストの `cfg!(windows)` の分岐の期待値を同じ文にする。kind と `(G10)` は変えない。
- 確認: `cargo test --locked --lib validates_shared_libraries_and_native_bindings`（1 件）。Windows で `target/debug/tsuzuri.exe build <lib> --emit shared` の文が新しい文になる。

### 手順 4: E2E

- 変更: `tests/e2e.mjs`、`tests/windows.mjs`、`tests/ffi_extensions.mjs`。
- 内容: `tests/e2e.mjs`（ci.yml の `suites` の job が Linux で実行する）の `failedCompiler` の段の後に、名前のない `extern def host_value :: unit -> i64` を呼ぶプロジェクトの `build --json` を足し、`E2002`、メッセージが `unresolved symbols: tsuzuri_host_Main_host_value` を含み `install LLVM/Clang` を含まないことを確かめる（GNU ld か ld.lld の実際の出力で R4 の判定が効くことを CI で固定する）。`tests/windows.mjs` にも同じ段（MSVC の link.exe）と、`--emit shared` の E2000 の文の検査（W05 が先なら省く）を足す。`tests/ffi_extensions.mjs` の 84 行の `rejected(["run", app], "E2002")` にも同じ 2 つの検査を足す（この suite は CI の step にないので手元の確認用）。
- 確認: `cargo build && node tests/windows.mjs target/debug/tsuzuri.exe`（Windows の Developer PowerShell）。`node tests/e2e.mjs target/debug/tsuzuri` と `node tests/ffi_extensions.mjs target/debug/tsuzuri` は Linux か macOS で（無ければ ci.yml の `suites` の結果）。

### 手順 5: 文書と comment

- 変更: 「ドキュメント」の節のページと `src/backend/target.rs` の comment。
- 内容: 仕様 R1〜R3・R5〜R7 の文を入れる。R3 の小節のコマンドは「現状」の実験のとおりに書き、確かめていない組み合わせ（同梱の MinGW の DLL を C のホストで動かすこと）は「MSVC で確認」と書き分ける。手元の Windows で R3 の手順を MinGW でも通したら、その旨を書いてよい。
- 確認: `node scripts/check-docs.mjs <変更したページ>`、`node scripts/check-cli-docs.mjs`、`git diff --stat` に挙げたファイル以外がない。

### 手順 6: 最終確認

- 変更: なし。
- 内容: T1（`cargo nextest run --locked`）と T2（変更したページの `check-docs`、`tests/windows.mjs`）。PR の説明に R1〜R7 と、変えなかった R6 の判定の理由を書く。
- 確認: すべて成功する。PR に `ci:full` を付ける（Windows の診断と `tests/windows.mjs` を変えるため）。

## テスト計画

- `undefined_symbol_hints_name_the_symbols`（`src/driver.rs`）: 入力と期待（名前の列）。lld-link の実際の出力（`lld-link: error: undefined symbol: GdiFlush` と `>>> referenced by ...`）→ `["GdiFlush"]`。link.exe の `tag.obj : error LNK2019: unresolved external symbol malloc referenced in function tz.string.to_well_formed` を 3 行（`malloc`・`free`・`realloc`）と `LNK1120` の行 → 3 つ。`LINK : error LNK2001: unresolved external symbol tsuzuri_alloc` → 1 つ。ld.lld の `ld.lld: error: undefined symbol: tsuzuri_host_Main_host_value`、GNU ld の ``host.c:(.text+0x5): undefined reference to `sqlite3_open'``、ld64 の 2 行の形、同じ名前の重複、9 個以上（`and 1 more`）。読まないもの（空）: `error: use of undefined value '@tz.fn.Main.shim_run'`（Clang の IR の誤り。W01 の再現）、`LNK1181: cannot open input file 'm.lib'`、`ld.lld: error: unable to find library -lfoo`、`clang: error: no such file or directory`、`wasm-ld: error: module.o: undefined symbol: tsuzuri_io_write`。
- `validates_shared_libraries_and_native_bindings`: Windows の分岐の文。
- E2E: 手順 4 の 3 つ（CI で固定されるのは `tests/e2e.mjs` と、`ci:full` の `tests/windows.mjs`）。
- 生成 IR は変わらない（このチケットは IR に触れない。`--emit llvm` の比較は要らない）。

## ドキュメント

- `docs/language.md`: 453 行（R1）、「リンク入力」（R7）、4965 行（R3）。
- `_tsuzuri/language-reference/compiler/platforms.md`: 166 行（R2）、169 行（R6）、既知の制限の R5 の 1 行。
- `_tsuzuri/language-reference/compiler/native-interop.md`: 230 行（R7）、「共有ライブラリと各言語のバインディング」の Windows の小節（R3）。
- `_tsuzuri/language-reference/compiler/option.md`: 15 行・433 行（R7）、270 行（R6）、517 行（R3）。
- `_tsuzuri/language-reference/organizing-tsuzuri/packages.md`: `[native] セクション`（R7）。
- `_tsuzuri/language-reference/built-in-types-and-modules/io.md`: R5 の段落。
- `docs/architecture.md`: 「高速デバッグバックエンド（PB05）」（R6）。
- `_tsuzuri/language-reference/compiler/diagnostics.md` は変えない（コードと kind が変わらない）。README は変えない（567 行の「Windows は未対応で `E2000`」は正しいまま）。

## 受け入れ条件

- [ ] R1〜R7 の各ページを仕様のとおりに直し、`node scripts/check-docs.mjs` が変更したページで通る。
- [ ] 未定義シンボルによるリンクの失敗の `E2002` が名前と `--link`・`-l`・`-L`・`[native]` を案内し、`install LLVM/Clang 17+` を出さない（Linux の `tests/e2e.mjs`（ci.yml）、Windows の `tests/windows.mjs`（`ci:full`））。ツールを起動できないときと wasm-ld の失敗の hint は変わらない。
- [ ] `undefined_symbol_hints_name_the_symbols` が成功し、読まないものの入力で空を返す。
- [ ] `--emit shared` の Windows の E2000 の文が R3 の文になり、テストと option.md の表が一致する（W05 が先に入ったときは対象外）。
- [ ] 言語の意味、生成 IR、診断のコードと kind、`link inputs require a native executable` の文が変わらない。
- [ ] 文書にチケット（`_features/W..`）へのリンクを書いていない（D6）。
- [ ] T2 が成功し、PR に `ci:full` を付けた。

## 落とし穴

- `tool_output` は Clang のコンパイルにも使うので、分類の語は「リンカーの未定義シンボル」の形に限る。`undefined value`（IR の誤り）や `undefined reference` だけの語では判定しない（GNU ld は ``undefined reference to ` `` まで見る）。
- link.exe の出力は CRLF。行の末尾の `\r` を名前に含めない（実際の出力は `\r\n` だった）。
- R3 の手順の `LIBRARY` は DLL のファイル名と同じにする。`.def` を使うと、import library が記録する DLL 名は `/OUT` ではなく `LIBRARY` になる（`/OUT:f_link.dll` でも `tag.dll` を指した）。
- R3 の (3) で `/IMPLIB:` を省くと、使えない import library が DLL と同じ名前で書かれ、(4) のものと取り違えやすい。手順では (4) の出力を後に書くか、(3) の import library を別名にする。
- R5 の再現で GUI の exe を起動するときは必ず `-Wait` で終わりを待つ（窓は出ない）。Defender が未署名の小さな GUI の exe を止めることがある（調査で 1 度あった）。
- R7 の `script` のオプションはファイルより前だけ。`script Main.tz --link x.o` の `--link` はプログラムの引数になる。

## 対象外

- `--emit shared` を Windows で実装すること（[W05](W05-windows-shared-library.md)）。
- `--backend fast` を Windows で有効にすること（[PB05](../_perfs/_completed/PB05-fast-debug-backend.md) の WP14 の後続）。
- 標準ハンドルのないときの `IO` の意味を決めること（[W08](W08-io-without-std-streams.md)）、トラップの報告を届けること（[W04](W04-windows-trap-report-lost.md)）。
- `ref` のない `string` の `E1008` に `ref string` の修正案（D-51 の `fixes`）を足すこと。
- C のソースをリンク入力として正式に受けること（コンパイルのオプションの決定とテストが要る機能。[E12](_completed/E12-ffi-extensions.md) D6 の拡張）。
- docs/language.md の「コールバック」節の寿命の文（[W12](W12-static-callback-lifetime.md)）。
- システムライブラリを `extern` ごとに求めること（[W17](W17-link-attribute-dllimport.md)）。この hint はその前後で同じ。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | `--backend fast` の Windows は文書が正しい。判定は変えず、architecture.md の注記と `src/backend/target.rs` の comment だけを足す | `BuildOptions::validate_fast` がホストと triple で拒否し（再現した）、COFF の書き出しは PB05 の WP14 のテストだけが使う。WP14 は「ホストでは E2000 のまま」と決め、翻訳器は `.pdata` を書かない | 既定案（実装者はこの案に従う） |
| D2 | DLL の回避策は「lld-link か MinGW で `.def` を使ってリンクし、import library は同じ `.def` から作り直す」 | link.exe は弱い定義を export できず、LLD の import library は `.weak.` の名前になる（どちらも確かめた）。export する別名の包み関数を書く案は、C#・Python のバインディングが `tsuzuri_free` を名前で呼ぶので採らない | 既定案（実装者はこの案に従う） |
| D3 | リンク入力の契約は object と静的ライブラリのまま（E12 D6）。C のソースは「Clang にそのまま渡り、検証していない」と書き、テストは足さない | テストのない振る舞いを契約にすると、コンパイルのオプション（`-O`、`--cpu native`、PDB の段）が実装の都合で変わったときに破れる。正式に受けるのは機能の追加で、このチケットの範囲を超える | 既定案（実装者はこの案に従う） |
| D4 | 未定義シンボルの判定は `tool_output` の 1 か所で行い、kind は `external_tool_failed` のまま hint の文だけを変える | 呼び出し側ごとに選ぶと `test`・`bench` の runner と codegen unit の経路を取りこぼす。新しい kind は JSON と `explain` を変えるが、利用者に要るのは文の案内だけ（D-51） | 既定案（実装者はこの案に従う） |
| D5 | 標準ハンドルのないプロセスの `IO` は既知の制限として今の振る舞いを書く | 意味を変えるのは W08（要承認）。文書は今の振る舞いだけを書く（AGENTS.md） | 既定案（実装者はこの案に従う） |
| D6 | 利用者向けの文書から、まだ終わっていない W チケットへリンクしない | チケットは完了すると `_completed/` へ移り、リンクが切れる（`check-docs` が落ちる） | 既定案（実装者はこの案に従う） |
