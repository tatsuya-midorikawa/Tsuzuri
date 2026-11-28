# W05: Windows の `--emit shared`（DLL）を実装する

| 項目 | 内容 |
| --- | --- |
| ID | W05 |
| 優先度 | P2（E13 Phase 2 の共有ライブラリと C#・Python・C++ のバインディングが Windows で使えない。今の回避策（object を DLL へつなぐ）は `tsuzuri_alloc`・`tsuzuri_free` を export できず、所有結果を返すライブラリが壊れる） |
| 規模 | L（driver の Windows のリンクの分岐、IR の 1 か所の書き換え、import library の sidecar とキャッシュ、3 つの E2E suite と CI の step、文書） |
| 依存 | – |
| 後続 | – |
| 状態 | doing |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 要承認（E13 の DLL の方針、G10 の後続）。D1（出力の形）は承認前に着手しない。ほかの決定は既定案 |
| 手本にする既存実装 | `src/driver.rs` の `link_shared`（macOS・Linux の分岐）・`shared_exports`・`pdb_link_args`・`pdb_path`・`BuildOptions::validate_shared`、`src/cache.rs` の `hash_output_path`、`src/llvm.rs` の `windows_abi`、`tests/windows.mjs`（object と IR を C のホストへつなぐ段）、`tests/host_bindings.mjs`、`tests/targets.mjs` の共有ライブラリの段 |
| 主な影響ファイル | `src/driver.rs`、`src/llvm.rs`、`src/cache.rs`、`tests/windows.mjs`、`tests/host_bindings.mjs`、`tests/targets.mjs`、`.github/workflows/windows.yml`（と冒頭の step の一覧の comment）、`_tsuzuri/language-reference/compiler/{platforms,native-interop,option}.md`、`_tsuzuri/language-reference/ownership-and-memory/stack-and-heap.md`、`docs/language.md`、`docs/architecture.md`、`README.md` |

## 実装者向け早見

- 範囲: Windows の target（ビルド機が Windows の MSVC と同梱の MinGW、ほかのホストからの `*-pc-windows-gnu`）の `--emit shared`。出力は DLL と、隣の import library `<stem>.lib`。export は macOS・Linux と同じ `shared_exports` の一覧。`--trap-mode return` は Windows では `E2000` のまま（対象外。外すのは [W35](W35-windows-trap-mode-return.md)）。
- 最初に読む: ① 「現状」の「確かめた実験」② 仕様 1〜4 ③ `src/driver.rs` の `link_shared` と `shared_exports` ④ `src/llvm_abi.rs` の `allocator`・`native_allocator`。
- 最初に書くテスト: `src/driver.rs` の単体テスト `windows_module_definitions_name_the_library_and_the_exports`、`src/llvm.rs` の `windows_shared_libraries_define_the_allocator_strongly`。その次に `tests/windows.mjs` の DLL と C のホスト。
- 触ってはいけない: macOS・Linux の `--emit shared` の出力、Windows の `exe`・`object`・`llvm`・`header`（IR を 1 byte も変えない）、C ヘッダー（`__declspec(dllimport)` を付けない。G10 の契約）、`shared_exports` の一覧、`link inputs require a native executable` の文（E13 Phase 2 の判断 8）。
- 完了の見分け方（MSVC の Developer PowerShell、ネイティブ連携のページの `make_tag` と `add` のライブラリ）:

```sh
tsuzuri build tag --emit shared -o tag.dll        # tag.dll と tag.lib ができる
llvm-nm tag.lib | grep __imp_                     # __imp_tsuzuri_alloc __imp_tsuzuri_free __imp_tz_add __imp_tz_make_tag
clang host.c tag.lib -o host.exe && ./host.exe    # ABC（tz_make_tag の結果を tsuzuri_free で解放する）
```

## 目的

E13 Phase 2 は、公開 C ABI だけを export する共有ライブラリ（`--emit shared`）と、それを呼ぶ C#・Python・C++ のバインディングを macOS と Linux に入れた。Windows は G10 の「DLL は含めない」（G10 D12）と E13 D11 の「G10 の Windows DLL 方針と揃える」の間で方針が決まらないまま `E2000` になっている。今の案内（object を DLL へつなぐ）は、`tsuzuri_alloc`・`tsuzuri_free` が弱い定義なので link.exe では export できず、LLD では使えない import library になる（W03 の R3）。このチケットは Windows の DLL の出力の形を決め、ほかの OS と同じ一覧を正しく export し、正しい import library を出す。

関連: [GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30)、[G10](_completed/G10-windows.md)（D8・D12、「後続の作業」）、[E13](_completed/E13-host-bindings.md)（D11、Phase 2 の判断 8・11）、[W03](W03-windows-interop-doc-fixes.md)（R3 の回避策）、[W07](_completed/W07-windows-trap-fail-fast.md)（`uwtable`）、[W10](W10-windows-resources.md)、[W17](_completed/W17-link-attribute-dllimport.md)、[W27](W27-win32-package-from-winmd.md)、[W30](W30-windows-hardening.md)。

## 着手条件と停止条件

### 着手条件

- D1 が承認されている。
- `node scripts/repo-kb.mjs brief W05` を読み、HEAD で `BuildOptions::validate_shared` の Windows の `E2000`、`src/llvm_abi.rs` の `allocator`・`native_allocator` の `define weak ptr @tsuzuri_alloc(`・`define weak void @tsuzuri_free(` の行が「現状」と同じことを確かめる。違えばこのチケットを直してから始める。
- W03 が先に入っていたら、W03 の R3（E2000 の文と native-interop.md の回避策の小節）は、このチケットの文書で置き換える。W03 が後なら W03 の R3 は飛ばされる。
- 手元に MSVC 14.44 と LLVM 21 以降（`clang`・`llvm-nm`・`llvm-readobj`）、同梱ツールチェーン。Python 3 と .NET 8 以降の SDK は手順 6 に要る（無ければ CI の結果を根拠にする）。

### 停止条件

- **S1** `tsuzuri_alloc`・`tsuzuri_free` のほかにも、export のために強い定義にしなければならない弱い定義が見つかった（D2 の書き換えが 2 行で済まない）。
- **S2** D2 の書き換えの後も、link.exe か同梱の MinGW の import library に `.weak.` の名前が残る、または DLL 名が出力のファイル名にならない（「現状」の実験と食い違う）。
- **S3** ランタイム入りの DLL（tzroutine・IO・Net）が、ホストからの呼び出し、ホストのスレッドの終了（TLS callback）、プロセスの終了のどれかで止まる・落ちる。選択肢: A そのランタイムを要する DLL を `E2002` にして先に進む、B 止めて報告する。2 回直して直らなければ B。
- **S4** C#・Python・C++ のバインディングが Windows の DLL で動かず、直すのに生成器（`src/bindings_native.rs`）の名前の規則（`{name}.dll`）以外の変更が要る。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

### ゲートとリンク

- `src/driver.rs` の `BuildOptions::validate_shared` は、Windows の target を `E2000`（kind `option_unsupported_platform`、`'--emit shared' is not supported on Windows yet (G10); build an object with --emit object and link it into a DLL`）にする。`validates_shared_libraries_and_native_bindings` の `cfg!(windows)` の分岐がこの文を固定する。
- `link_shared` は `platform.mach_o()`（`-dynamiclib`・`-exported_symbols_list`・install name）とそれ以外（`-shared`・version script・`-soname`・`--no-undefined`）の 2 分岐だけで、ランタイムがあれば無条件に `-pthread`、Linux で `-ldl`、常に `-lm` を足す。Windows の exe のリンク（`build_complete`）は `-lm`・`-pthread` を足さず、Net のランタイムに `-lws2_32` を足す。
- `shared_exports` は IR の `define` の行から `tz_<name>`（`--trap-mode return` なら `tsuzuri_try_<name>`）と、定義されていれば `tsuzuri_alloc`・`tsuzuri_alloc_stats`・`tsuzuri_free`・`tsuzuri_main` を、`internal`・`hidden` を除いて並べる（弱い定義も入る）。
- `llvm::windows_abi` は `tz_*` と、入口があれば `tsuzuri_main` の `define` にだけ `dllexport` を付ける。`src/llvm_abi.rs` の `allocator`・`native_allocator` は `define weak ptr @tsuzuri_alloc(i64 %size) nounwind {` と `define weak void @tsuzuri_free(ptr %value) nounwind {` を出す。`tsuzuri_alloc_stats`（`src/runtime/heap-counting.ll`）は強い定義。
- 実行ファイルの段の `artifact` は一時ディレクトリの `artifact`（Windows の exe は `artifact.exe`）で、`publish_outputs` が出力のパスへ移す。PDB は `pdb`（`options.emit == Emit::Executable && msvc_linker_for(platform)` のときだけ）と `pdb_link_args`（`-Xlinker /PDB:` など。`-Wl,` はパスのコンマで割れる）。
- codegen unit は `--emit shared` を分けない（`src/driver/units.rs` の `eligible` は native の `Executable`・`Object` と wasm32 の `Wasm` だけ）。

### 確かめた実験（`export def make_tag :: i64 -> [ubyte]` と `export def add` のライブラリ、MSVC 14.44・LLVM 22.1.8・同梱 0.1.6）

| 手順 | 結果 |
| --- | --- |
| `--emit object` の記号 | `W tsuzuri_alloc`・`W tsuzuri_free`（実体は `.weak.tsuzuri_alloc.default.tz_make_tag`） |
| `link.exe /DLL`（export の指定なし） | export は `tz_add`・`tz_make_tag` だけ |
| `link.exe /DLL /EXPORT:tsuzuri_free ...`、`/DEF:tag.def` | どちらも `LNK2001: unresolved external symbol tsuzuri_alloc`。link.exe は弱い外部シンボルを export できない |
| `lld-link /DLL /DEF:tag.def`、同梱の `tsuzuri-clang -shared tag.o tag.def -Wl,--out-implib,...` | export 表は正しいが、import library の名前は `__imp_.weak.tsuzuri_free.default.tz_make_tag`。それでリンクした C のホストは `undefined symbol: tsuzuri_free` |
| IR の 2 行を `define dllexport ptr @tsuzuri_alloc(`・`define dllexport void @tsuzuri_free(` に変えて、link.exe（`.def` あり・なし）と同梱の MinGW（`.def` を入力に） | export 表も import library も `__imp_tsuzuri_alloc`・`__imp_tsuzuri_free`・`__imp_tz_add`・`__imp_tz_make_tag` |
| `.def` の `LIBRARY tag` と `/OUT:f_link.dll` | import library は `tag.dll` を指し、link.exe は `LNK4070`（`/OUT` の名前が `.def` と違う）を出す。`.def` なしの `/OUT:e_link.dll` は `e_link.dll` を指す |
| 強い定義の object で、`LIBRARY "tag.dll"`（引用符と拡張子つき）の `.def` と `-o <dir>/tag.dll`。MSVC は `clang -shared -Xlinker /DEF:<def> -Xlinker /IMPLIB:<dir>/tag.lib`（link.exe と `-fuse-ld=lld` の両方）、同梱の MinGW は `tsuzuri-clang -shared -x none <obj> <def> -Xlinker --out-implib=<dir>/tag.lib` | 3 つとも警告なしで成功し、import library は `__imp_` の 4 つの名前と `tag.dll`。`LIBRARY "my lib.dll"` と空白を含むパスも 3 つとも `my lib.dll` を指す。同梱の MinGW は出力の隣に `tag.pdb` も書いた（`-g` なし）。MSVC の出力のディレクトリに `.exp` は残らなかった |
| `tests/fixtures/tzroutine/Main.tz` の `--emit llvm -O0` を `src/runtime/tzroutine.c` と `clang -shared -Wl,/DEF:tz.def` で DLL にし、`tests/windows.mjs` と同じ C のホスト（`tz_sum_atomic(100) == 4950` など 5 つ）を `tz.lib` でリンク | 成功（終了コード 0）。tzroutine のランタイムは DLL の中で動く |
| MSVC の DLL の import | `KERNEL32.dll` だけ（Clang の既定で CRT は静的な `libcmt`） |

### ランタイムとトラップ

- `src/runtime/trap.c`（`--trap-mode return` と `Domain.run` のランタイム）は `pthread.h` を無条件に読み、`setjmp`・`longjmp` で戻る。Windows の生成コードは `.pdata` を持たない（W07 の範囲）。`src/test_runner.rs` の `compile_native_runner` は Windows でトラップのランタイムを `domain_on_windows`（E1038）にする。`--emit object --trap-mode return` は Windows で `E2002`（`build_complete` の COFF のランタイムの判定）。
- `src/runtime/tzroutine.c` は Windows でホストのスレッドの擬似 G を TLS callback（`.CRT$XLB` の `tzr_tls_callback`、`DLL_THREAD_DETACH`）で片付け、スケジューラーは最初の呼び出しで起動する（`tzr_start` の `tzr_run_once`）。`trap.c` の `__attribute__((constructor))` はポインターを代入するだけ。どのランタイムも `DllMain` を持たない。

### キャッシュ、バインディング、テスト、文書

- `src/cache.rs` の `hash_output_path` は、`Emit::Shared` の出力のファイル名（`shared-library-name`）を鍵に入れる。Windows の PDB の名前（`pdb-name`）は `Emit::Executable` だけ。
- `src/bindings_native.rs`: Python の `load` は Windows で `{}.dll` を探し、C# の `LibraryName` の doc comment は `{name}.dll` の探索を書いている（どちらも Windows の DLL を前提にした名前を既に持つ）。
- `tests/host_bindings.mjs` は POSIX だけを前提にする（拡張子 `dylib`／`so`、`lib` の接頭辞、`nm -D`、ホストの `-fPIC`）。CI では `targets.yml` の `strings-tools` の job の step「Compact strings (PM03)」が `tests/compact_strings_modes.mjs ... quick`（一覧に `host_bindings` がある）として実行する。この機械には Python も .NET もなく、バインディングの実行は確かめていない。`tests/targets.mjs` は Windows の target の共有ライブラリを飛ばし（`if (target.os !== "windows")`）、`inspect` は PE をすべて `executable` と読む。`tests/windows.mjs` に DLL の段はない。
- Windows の DLL を未対応と書く所: `platforms.md:83`（表の「なし（`E2000`）」）・`165`、`native-interop.md:277`、`option.md:97`・`517`、`docs/language.md:4965`、`README.md:567`、`stack-and-heap.md:265`（「Windows の `shared` は試験していません」）。

## 変えてはいけない意味

- macOS・Linux の `--emit shared` と、Windows の `exe`・`object`・`llvm`・`header`・`bitcode`（`E2000` のまま）の出力。D2 の書き換えは Windows の `--emit shared` の IR だけに効く。
- export の一覧（`shared_exports`）、所有結果の契約（`tsuzuri_free` で 1 回だけ解放）、トラップがプロセスを終わらせること（`0xC000001D`）。
- C ヘッダー（`dllimport` なし。関数の呼び出しは import library の thunk を通る）。

## 仕様

### 1. 出力

- `tsuzuri build <lib> --emit shared [-o <name>.dll]` は、Windows の target で `<name>.dll` と、同じディレクトリの import library `<name>.lib` を書く（D1）。既定の出力名は今と同じ `<入力>.dll`（`BuildOptions::output_path`）。
- 出力の拡張子は `.dll`（大文字と小文字を問わない）でなければならない。違えば `E2000`（kind `output_extension_invalid`、文案 `a Windows shared library must end with '.dll'; its import library is written next to it as '<stem>.lib'`。D9）。
- `.def`・`.exp`・リンカーの中間物は書かない（一時ディレクトリに残して消す）。
- `-g`: MSVC のリンカー（`msvc_linker_for(platform)`）は `<name>.pdb` を DLL の隣に書く（exe と同じ `pdb_link_args`）。同梱の MinGW は exe と同じく DWARF を DLL に持つ（D5）。

### 2. export と import library

- DLL の export は `shared_exports` の一覧と同じ（`tz_*`、定義されていれば `tsuzuri_alloc`・`tsuzuri_free`・`tsuzuri_main`・`tsuzuri_alloc_stats`）。ランタイムとリンクしたホストの関数は export しない。
- import library の記号は、その一覧の `__imp_<名前>` と `<名前>` だけで、記録する DLL 名は出力のファイル名。
- ビルドキャッシュから返すときも、DLL と import library（と `-g` の PDB）の組を返す。

### 3. ランタイムとオプション

- tzroutine・CPU・IO・Net のランタイムは、実行ファイルと同じ object を DLL に入れる（Net は `-lws2_32`）。OS API は今と同じ `E2002`（`build_complete` の `os_message`）。
- `--trap-mode return` は Windows の `--emit shared` で `E2000`（kind `option_unsupported_platform`、文案 `--trap-mode return is not supported for Windows shared libraries yet: its runtime needs POSIX threads and unwind tables; build without --trap-mode return`。D4。[W35](W35-windows-trap-mode-return.md) が先に入っていれば、D4 のとおりこの `E2000` は入れずに受ける）。`Domain.run` は今と同じ `E1038`。
- `--allocator counting`・`small` は今の共有ライブラリと同じく受ける。`host` は今と同じ `E2000`。`--link`・`-l`・`-L`・`[native]` を受ける。

### 4. ホストへの約束（文書に書く）

- DLL は自分の CRT を持つ（MSVC は静的な `libcmt`、同梱の MinGW は UCRT）。所有結果は必ず `tsuzuri_free` で解放する（今の契約のまま。ホストの `free` は別のヒープ）。
- `FreeLibrary` で DLL を外すことは対象外（tzroutine の M、`Task.parallel` の常駐のスレッドが DLL のコードを走り続ける）。プロセスの終わりまで読み込んだままにする（D7）。
- C#・Python のバインディングは `<name>.dll` を名前で読み、C・C++ のホストは `<name>.lib` でリンクする。ヘッダーは `dllimport` を付けない（D8）。

## 設計

### ゲート（`BuildOptions::validate_shared`）

```rust
if self.platform().windows() && self.trap_return {
    return Err(driver_error_of(DiagnosticKind::OptionUnsupportedPlatform, "--trap-mode return is not supported for Windows shared libraries yet: ..."));
}
```

Windows の target の無条件の `E2000` を外し、上の判定に置き換える。拡張子の判定（仕様 1）は出力のパスが要るので、`bindings_output_error` と同じく `src/main.rs` の引数の検査の後か `build_complete` の先頭（`protect_sources` の前）に置く。

### IR（`src/llvm.rs`）

```rust
/// W05: the copy of tsuzuri_alloc and tsuzuri_free that a Windows DLL exports. link.exe cannot export
/// a weak definition and LLD names the import of one `.weak.<name>.default.<symbol>`, so the DLL
/// defines them strongly. Only a Windows `--emit shared` build calls this; every other output keeps
/// the weak definitions that let several Tsuzuri objects share one host.
pub(crate) fn with_exported_allocator(ir: String) -> String
```

- `build_complete` の `windows_abi` の直後で、`options.emit == Emit::Shared` のときだけ呼ぶ。行がちょうど `define weak ptr @tsuzuri_alloc(i64 %size) nounwind {`・`define weak void @tsuzuri_free(ptr %value) nounwind {` のときに `weak` を `dllexport` に変える。ほかの行は変えない（`tsuzuri_arguments`・`tz_soft_*`・`tz_small_*`・`tsuzuri_trap_hooks` の弱い定義は DLL の中で解決するだけで export しない）。[W39](W39-windows-multiple-objects-weak-defaults.md) が先に `main` に入っていれば、Windows の弱い定義の行の末尾に ` comdat` が付くので、行全体の一致ではなく「`define weak ` で始まり `@tsuzuri_alloc(`・`@tsuzuri_free(` を含む行」の `weak` を `dllexport` にし、` comdat` は残す（`dllexport` と `comdat` の組が link.exe と lld-link の両方で正しく export されることは、W39 の起票時に確かめた）。
- codegen unit は Shared を分けないので、書き換えた定義はいつも 1 つの module にある（分けるようになったら停止条件 S1 と同じく報告する）。

### `.def` とリンク（`src/driver.rs`）

```rust
/// W05: the module-definition file of a Windows DLL: its file name, so that the import library
/// names the DLL that is published, and the exports of `shared_exports`.
fn module_definition(library: &str, symbols: &[String]) -> String; // "LIBRARY \"tag.dll\"\nEXPORTS\n    tsuzuri_alloc\n..."
```

`link_shared` に Windows の分岐を足す（引数に Net のランタイムの有無、import library と PDB の置き場を足す）。旗の書き方は、リンカーの種類で選ぶ。MSVC の ABI で、Clang が同梱の launcher でない（`platform.msvc() && !bundled_launcher(clang)`。`codeview_for` と同じ条件）なら link.exe・lld-link の書き方、それ以外（同梱の launcher は MSVC の指定も MinGW でリンクする、`*-pc-windows-gnu`）なら MinGW の書き方にする。ホストで決まる `msvc_linker_for` は PDB にだけ使う（Windows 以外のホストのシステムの Clang が lld-link で MSVC の target をリンクするとき、MinGW の書き方を渡さないため。その組み合わせは exe と同じく未検証）。

| | link.exe・lld-link（MSVC の ABI、launcher でない） | MinGW（同梱の launcher、`*-pc-windows-gnu`） |
| --- | --- | --- |
| 共有ライブラリ | `-shared` | `-shared` |
| export の一覧 | `-Xlinker /DEF:<一時>/exports.def` | `-x none` の後に `exports.def` を入力として渡す |
| import library | `-Xlinker /IMPLIB:<一時>/shared/<stem>.lib` | `-Xlinker --out-implib=<一時>/shared/<stem>.lib`（`-Wl,` はパスのコンマで割れる） |
| `-g` | `msvc_linker_for(platform)` なら `pdb_link_args` | `-g`（DWARF） |
| ランタイム | object、Net は `-lws2_32` | 同じ |
| 足さないもの | `-lm`・`-pthread`・`-ldl`・version script・`-soname`・`--no-undefined`（Windows のリンクは未解決の記号をいつも誤りにする） | 同じ |

- 段の DLL は一時ディレクトリの `shared/<出力のファイル名>` に置く（`artifact` の決め方に Windows の Shared の場合を足す）。`/OUT` と `LIBRARY` の名前がそろい、`LNK4070` を出さない。`.exp` や、同梱の zig が書く `.pdb`（実験で見た）ができても一時ディレクトリに残り、出力には出さない。
- `LIBRARY` の名前は引用符で囲む（空白を含むファイル名）。export の名前は ASCII の識別子だけ（docs/language.md の識別子の規則）なので引用しない。

### sidecar とキャッシュ

- `import_library = (platform.windows() && options.emit == Emit::Shared).then(|| output.with_extension("lib"))`。`protect_sources`・`protect_links` を PDB と同じく当て（`--link foo.lib` と同じパスへの上書きを `E2003` にする）、`cache_paths` に `"import"`、`publish_outputs` の sidecar に足す。
- `pdb` の条件を `matches!(options.emit, Emit::Executable | Emit::Shared)` にし、`src/cache.rs` の `hash_output_path` の `pdb-name` も Shared に広げる。import library の DLL 名は `shared-library-name` が既に鍵に入れている。

## 前提とする他チケットのインターフェース

- なし。W03 の R3 と同じ行（`validate_shared` の文、native-interop.md の Windows の文、option.md:517、docs/language.md:4965）を書き換えるので、後に入る方が rebase で置き換える。

## 他チケットへ提供するインターフェース

- `link_shared` の Windows の分岐: W10（リソース）が DLL に版情報を埋め込むときの入口。W30（硬化）が `/guard:cf` などのリンクの旗を DLL にも足すときの入口。
- `with_exported_allocator` と `module_definition`: W27（生成した `win32` パッケージ）や E13 の後続が DLL を配るときの前提。
- `--trap-mode return` の Windows の DLL の `E2000`: W07（`uwtable`）と `trap.c` の Windows の移植が済んだ後に外す後続の作業の印。外すのは [W35](W35-windows-trap-mode-return.md)（D6）。

## 実装手順

### 手順 1: ベースライン

- 変更: なし（下書きは `target/scratch/w05/` の下）。
- 内容: 「確かめた実験」を HEAD でもう一度行う（object の記号、link.exe の `LNK2001`、LLD の import library の名前、IR の 2 行を変えたときの結果）。同梱の launcher が `-Xlinker --out-implib=...` と `.def` の入力を zig へ渡すことを確かめる。`examples/` と `tests/fixtures/` のすべての `Main.tz` の Windows の `--emit llvm -O0`・`-O3` を保存する（手順 3 の比較用）。
- 確認: 実験の結果が表と同じ。違えば停止条件 S2。

### 手順 2: ゲートと診断

- 変更: `src/driver.rs`（`validate_shared`、拡張子の判定、`validates_shared_libraries_and_native_bindings`）。
- 内容: 仕様 1・3 の `E2000`。
- 確認: `cargo test --locked --lib validates_shared_libraries_and_native_bindings`（1 件）。T0。

### 手順 3: IR の書き換え

- 変更: `src/llvm.rs`（`with_exported_allocator` と単体テスト）、`src/driver.rs`（`windows_abi` の直後の呼び出し）。
- 内容: 「設計」の IR の節。書き換える 2 行を定数にし、`allocator`・`native_allocator` の出力に必ず現れることを単体テストで固定する（文が変わったら書き換えが黙って効かなくなるのを防ぐ）。呼び出しは `platform.windows() && options.target == Target::Native && options.emit == Emit::Shared` のときだけ。
- 確認: `cargo test --locked --lib windows_shared_libraries_define_the_allocator_strongly`。手順 1 で保存した IR と今の Windows の `--emit llvm` を `cmp` し、すべて一致する。

### 手順 4: `.def`、リンク、sidecar、キャッシュ

- 変更: `src/driver.rs`（`module_definition`、`link_shared` の Windows の分岐、`artifact` の段の名前、`import_library`、`pdb`）、`src/cache.rs`（`hash_output_path`）。
- 内容: 「設計」の `.def` とリンク、sidecar とキャッシュの節。`module_definition` の文字列は `shared_exports` の順（並べ替え済み）にして決定的にする。リンクの hint は `link_shared` の今の文（`unresolved extern symbols need --link, -l or -L`）を使う。
- 確認: `cargo test --locked --lib windows_module_definitions_name_the_library_and_the_exports`。手元で「完了の見分け方」のコマンドが MSVC と同梱の MinGW の両方で通る。2 回目の `build` がキャッシュから DLL・import library を返し、内容が 1 回目と同じ。`TSUZURI_CLANG` を同梱の launcher にした MSVC の target（`--target native`）で MinGW の書き方が選ばれる。

### 手順 5: `tests/windows.mjs`

- 変更: `tests/windows.mjs`。
- 内容: 「テスト計画」の E2E。
- 確認: `cargo build && node tests/windows.mjs target/debug/tsuzuri.exe`（Developer PowerShell、`TSUZURI_CLANG` は LLVM の Clang）。

### 手順 6: バインディングと CI

- 変更: `tests/host_bindings.mjs`、`.github/workflows/windows.yml`（新しい step `node tests/host_bindings.mjs target/release/tsuzuri.exe`、`PYTHON=python`。冒頭の step の一覧の comment）。`.github/instructions/ci.instructions.md` を先に読む。
- 内容: Windows では、ライブラリを `native.dll`（`lib` の接頭辞なし。Python と C# が `{name}.dll` を探す）、export を `llvm-readobj --coff-exports` で読み、ホストの C を `-fPIC` なしで `host.obj` にし、C++ は `native.lib` でリンクして DLL を実行ファイルの隣に置く。`--trap-mode return` の段は `E2000` を確かめて飛ばす。install name の検査は import library の DLL 名の検査に替える。
- 確認: Python・.NET のある機械で `node tests/host_bindings.mjs target/debug/tsuzuri.exe`。無ければ CI（`ci:full`）の結果（起票の時点で、Python と .NET での Windows の DLL の実行は確かめていない）。POSIX の結果は `targets.yml` の「Compact strings (PM03)」の step で変わらないこと。

### 手順 7: `tests/targets.mjs`

- 変更: `tests/targets.mjs`。
- 内容: 同梱ツールチェーンのクロスリンクの段で、`*-pc-windows-gnu` の `--emit shared` を飛ばさない（`if (target.os !== "windows")` を外す）。出力名は `answer-<name>.dll`（仕様 1 の拡張子）。`inspect` は PE の COFF header の Characteristics（PE の署名の後 4 + 18 バイト）の `IMAGE_FILE_DLL`（0x2000）で DLL を見分け、`type: "dll"` を返す。隣の `answer-<name>.lib` があることも確かめる。
- 確認: macOS・Linux の CI（`targets.yml`。`ci:full`）。`aarch64-pc-windows-gnu` の DLL のリンクはこの機械で確かめていない（未確認）。失敗したら、その target の DLL を `E2002` にするかを報告する。

### 手順 8: 文書と最終確認

- 変更: 「ドキュメント」の節のページ。W03 の R3 の小節が入っていれば消す。
- 内容: 仕様 1〜4 を利用者向けに書く（出力の組、`.dll` の拡張子、import library、CRT と `tsuzuri_free`、`FreeLibrary`、`--trap-mode return` の `E2000`）。確かめた範囲は「MSVC（windows.yml の x64・Arm64）で実行、MinGW はクロスリンク（targets.yml）と手元」のように実際の検査だけを書く。
- 確認: `node scripts/check-docs.mjs <変更したページ>`、`node scripts/check-cli-docs.mjs`、T1（`cargo nextest run --locked`）、T2（`tests/windows.mjs`、Windows の `tests/host_bindings.mjs`）。PR に `ci:full` を付け（Windows の出力と CI を変える）、`windows.yml`・`targets.yml` の結果を読む。

## テスト計画

### Rust（`src/driver.rs`・`src/llvm.rs` の単体テスト）

- `windows_module_definitions_name_the_library_and_the_exports`: `module_definition("tag.dll", &[...])` が `LIBRARY "tag.dll"`、`EXPORTS`、名前の順の行。空白を含む名前（`my lib.dll`）は引用符の中。
- `windows_shared_libraries_define_the_allocator_strongly`: `allocator()` と `native_allocator()` の出力を通すと 2 行だけが `dllexport` になり、ほかの行と `tsuzuri_arguments` などの弱い定義は変わらない。書き換えを通さない Windows の `--emit llvm` は今と同じ（`define weak`）。
- `validates_shared_libraries_and_native_bindings`: Windows の分岐を「受ける。`--trap-mode return` は新しい `E2000`。`--allocator host` は今の `E2000`」に直す。拡張子の `E2000` の単体テスト。
- `shared_libraries_export_the_public_entry_points_the_ir_defines`: `define dllexport ptr @tsuzuri_alloc(` の行も今と同じ一覧になる入力を足す。

### E2E（`tests/windows.mjs`、`-O0` と `-O3`）

| 名前 | 内容 | 期待 |
| --- | --- | --- |
| 出力の組 | `make_tag`・`add` のライブラリを `--emit shared -o tag.dll` | `tag.dll`・`tag.lib` があり、`tag.exp`・`.def` は出力のディレクトリにない |
| export | `llvm-readobj --coff-exports tag.dll` | `tsuzuri_alloc`・`tsuzuri_free`・`tz_add`・`tz_make_tag` だけ（`--allocator counting` なら `tsuzuri_alloc_stats` も） |
| import library | `llvm-nm tag.lib` と `llvm-readobj tag.lib` | `__imp_` の名前が上の一覧と同じで `.weak.` がない。DLL 名は `tag.dll` |
| C のホスト | ネイティブ連携のページの `make_tag` のホストを `tag.lib` でリンクして実行 | `ABC`、終了コード 0 |
| ランタイム入り | `tests/fixtures/tzroutine` を `--emit shared`、既存の段と同じ 5 つの assert のホスト | 終了コード 0。ホストのスレッドを 8 本作り、それぞれ `tz_sum_atomic` を呼んで終える（TLS callback の片付け）ものも 0 |
| `-g` | `-g --emit shared` | `tag.pdb` があり、DLL がその名前を指す |
| キャッシュ | 同じ `build` を 2 回 | 2 回目の DLL・import library が 1 回目と同じバイト列 |
| 拒否 | `--trap-mode return`（W35 が先に入っていれば受ける。D4）、`-o tag.so`、`--allocator host` | `E2000`（それぞれの文） |
| 未解決の extern | `extern def` の定義を `--link` に渡さない | `E2002` |

### E2E（ほかの suite）

- `tests/host_bindings.mjs`（Windows の step）: C++・Python・C# の通常の段（`--trap-mode return` の段は飛ばす）。`--allocator counting` の確保の追跡が 0 で終わる。
- `tests/targets.mjs`: `x86_64-pc-windows-gnu`・`aarch64-pc-windows-gnu` の DLL のリンク（実行はしない）。

### 性能

- 計らない（リンクの形の追加で、生成コードは書き換えた 2 つの関数の linkage だけが違う）。

## ドキュメント

- `_tsuzuri/language-reference/compiler/platforms.md`: 83 行の表（`.dll` と import library の `.lib`）、165 行の既知の制限から `--emit shared` を外し、`--trap-mode return` の DLL と `FreeLibrary` を足す。「確かめた範囲」に Windows の DLL の検査（windows.yml、targets.yml のリンク）。
- `_tsuzuri/language-reference/compiler/native-interop.md`: 冒頭の検証の文（5 行）、「共有ライブラリと各言語のバインディング」（277 行。Windows の出力の組、`LIBRARY`、CRT、`FreeLibrary`、ヘッダーと import library、C#・Python の `{name}.dll`、`--trap-mode return` の `E2000`）。W03 の回避策の小節があれば消す。
- `_tsuzuri/language-reference/compiler/option.md`: 97 行、383 行（`--trap-mode return` の Windows の DLL）、517 行の表の行を新しい 2 つの `E2000` に置き換える。
- `_tsuzuri/language-reference/ownership-and-memory/stack-and-heap.md:265`。
- `docs/language.md:4965`、`docs/architecture.md:224`（`link_shared` の Windows の分岐、`.def`、強い定義）と 288 行（キャッシュの鍵の PDB の名前）。
- `README.md:567`（Windows の DLL）と 918 行（windows.yml の step）。
- `node scripts/check-docs.mjs <変更したページ>`、`node scripts/check-cli-docs.mjs`。

## 受け入れ条件

- [ ] Windows の `--emit shared` が DLL と `<stem>.lib` を書き、export と import library の名前が `shared_exports` の一覧と同じで、`.weak.` の名前がない（MSVC と同梱の MinGW）。
- [ ] import library が記録する DLL 名が出力のファイル名で、`LNK4070` などの警告を出さない。
- [ ] C のホストが import library でリンクでき、所有結果を `tsuzuri_free` で解放できる。ランタイム入りの DLL がホストのスレッドから呼べる。
- [ ] `--trap-mode return`、`.dll` でない出力、`--allocator host` が仕様の `E2000`（`--trap-mode return` は、W35 が先に入っていれば D4 のとおり受ける）。新しい診断コードも kind も足していない。
- [ ] Windows の `exe`・`object`・`llvm`・`header` と、macOS・Linux の `--emit shared` の出力が変わらない（手順 3 の `cmp`、`tests/host_bindings.mjs` の POSIX の結果）。
- [ ] ビルドキャッシュが DLL・import library・PDB の組を返す。
- [ ] `tests/windows.mjs`、Windows の `tests/host_bindings.mjs`（C++・Python・C#。`--trap-mode return` の段を除く）、`tests/targets.mjs` の MinGW の DLL が CI で成功する。
- [ ] 文書を直し、`check-docs`・`check-cli-docs` が通る。T2 が成功し、PR に `ci:full` を付けた。

## 落とし穴

- **`LIBRARY` と段の名前**: `.def` を使うと、import library の DLL 名は `/OUT` ではなく `LIBRARY` になる。段の名前が `artifact` のままだと `LNK4070` が出て、`.def` がなければ `artifact` を指す import library になる。段の DLL を出力のファイル名で置く。
- **弱い定義の export**: `.def` に書いても link.exe は弱い定義を export できない（`LNK2001`）。`.def` だけで直そうとしない。書き換えは Windows の Shared だけにする（ほかの出力の弱い定義は、複数の Tsuzuri の object を 1 つのホストに入れるためにある）。
- **`-Wl,` とコンマ**: パスは `-Xlinker` で渡す（`pdb_link_args` の comment と同じ理由）。
- **ランタイムの初期化は遅らせたまま**: `DllMain` やコンストラクターでスレッドを作らない（loader lock）。今のランタイムは最初の呼び出しで起動し、`trap.c` のコンストラクターは代入だけ。DLL でも TLS callback は loader lock の下で動くので、そこに新しいロックを足さない。
- **CRT のヒープ**: MSVC の DLL は静的な CRT の自分のヒープを持つ。ホストが `free` で解放すると壊れる（`tsuzuri_free` の契約を文書の目立つ所に書く）。
- **`-lm`**: link.exe は `m.lib` を探して `LNK1181` になる。Windows の分岐では足さない。
- **`.exp` と `.def` を出力に残さない**: `/IMPLIB:` の隣に `.exp` ができる。一時ディレクトリに置く。
- **Arm64**: windows.yml は `windows-11-arm` でも動く。`.def` は機械に依らないが、import library の機械はリンカーが決める。Arm64 の結果も確かめる。

## 対象外

- `--trap-mode return` と `Domain.run` の Windows の DLL（`trap.c` の Windows の移植と W07 の `uwtable` の後）。[W35](W35-windows-trap-mode-return.md) の Phase 1（`--trap-mode return`）と Phase 2（`Domain.run`）が扱う。
- `FreeLibrary` に耐える DLL（ランタイムのスレッドの停止）、`DllMain` のフック、遅延読み込み。
- DLL のリソース・版情報（W10）、署名、硬化の旗（W30）。
- C ヘッダーの `__declspec(dllimport)`、データの export。
- Windows 以外のホストで MSVC の target の DLL をリンクすること（exe と同じく未検証。同梱ツールチェーンは `E2002`）。
- MinGW の慣習の名前（`lib<stem>.dll.a`）の import library、`.def` の公開。
- `--emit bitcode` の Windows（`E2000` のまま）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | Windows の `--emit shared` は DLL と、隣の import library `<stem>.lib` を出す。export は macOS・Linux と同じ一覧 | E13 D11 は「G10 の Windows DLL 方針と揃える」とし、G10 D12 は DLL を含めなかったので、方針の記録がない。import library がないと C・C++ のホストはリンクできず、一覧を変えるとバインディングが OS ごとに分かれる | 要承認（承認前は該当する段に着手しない） |
| D2 | Windows の DLL でだけ、`tsuzuri_alloc`・`tsuzuri_free` を強い `dllexport` の定義にする | link.exe は弱い定義を export できず、LLD は使えない import library を書く（確かめた）。強い定義なら両方のリンカーで正しい。退けた案: import library を `.def` から `lib`・`llvm-dlltool`・`zig dlltool` で作り直す（link.exe の export は直らず、ツールが 1 つ増える）。DLL は 1 つの module と 1 つのランタイムだけを含むので、弱い定義の役目（複数の object の統合）がない | 既定案（実装者はこの案に従う） |
| D3 | export の一覧は `shared_exports` から `.def` を生成して渡し、`LIBRARY` は出力のファイル名、段の DLL も同じ名前に置く | Linux の version script と同じ役目。`/EXPORT:` の並びは `LIBRARY` を決められず、MinGW と書き方が分かれる | 既定案（実装者はこの案に従う） |
| D4 | `--trap-mode return` の Windows の DLL は `E2000` | `trap.c` は `pthread.h` を要し、`longjmp` は `.pdata` のない Windows のフレームを越えられない（G10 の後続の一覧、W07）。黙ってコンパイルの失敗（`E2002`）にしない。後続の [W35](W35-windows-trap-mode-return.md) がこの `E2000` を外す（W35 の D6。2 つの理由は W35 の D2 と W07 で消える）。W35 が先に `main` に入っていれば、この `E2000` は入れず、W35 の仕様 4 のとおり受け、W35 の手順 5 の DLL の段をこのチケットの PR で足す | 既定案（実装者はこの案に従う） |
| D5 | `-g` は MSVC のリンカーで `<stem>.pdb`、MinGW は DWARF | exe と同じ扱い（`pdb_link_args`）。PDB の名前は鍵に入れる | 既定案（実装者はこの案に従う） |
| D6 | CRT は exe と同じ選び方（MSVC は Clang の既定の静的 CRT） | 依存の DLL を増やさずに配れる（import は `KERNEL32.dll` だけだった）。所有結果は既に `tsuzuri_free` で解放する契約 | 既定案（実装者はこの案に従う） |
| D7 | `FreeLibrary` は対象外と文書に書く | ランタイムのスレッドは止まらない。POSIX の `dlclose` も同じだが、どの OS の文書にも書いていないので、Windows の節と共有ライブラリの節に書く | 既定案（実装者はこの案に従う） |
| D8 | C ヘッダーは変えない（`dllimport` なし） | G10 の契約（object を直接リンクする利用者）を保つ。関数は import library の thunk で呼べる | 既定案（実装者はこの案に従う） |
| D9 | Windows の共有ライブラリの出力は `.dll` で終わる。違えば `E2000`（`output_extension_invalid`） | import library の名前 `<stem>.lib` との衝突を防ぎ、バインディングが探す `{name}.dll` と合わせる | 既定案（実装者はこの案に従う） |
| D10 | 同梱の MinGW でも import library の名前は `<stem>.lib` | 1 つの名前で両方のホストに配れる。MinGW の lld もパスで渡せば読む（`-l` の探索名は確かめていない） | 既定案（実装者はこの案に従う） |
| D11 | 新しい診断コードを作らない。既存の kind `option_unsupported_platform`・`output_extension_invalid`（E2000）とリンクの `E2002` を使う | D-51 と E13 D12 | 既定案（実装者はこの案に従う） |
