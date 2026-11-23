# W39: Windows で 2 つの Tsuzuri の object を 1 つのホストにリンクできない（弱い定義の既定の名前が object ごとに違う）

| 項目 | 内容 |
| --- | --- |
| ID | W39 |
| 優先度 | P2（ランタイムを要しない object でも、MSVC の 2 つのリンカー（lld-link・link.exe）では 2 つ目の object が必ず失敗する。回避策は 1 つのホストに object を 1 つにすることだけ。同梱の MinGW では今もリンクできる） |
| 規模 | M（`src/llvm.rs` の Windows の書き換えに 1 つの関数、条件付きで `src/backend/module.rs` の 1 か所、Rust のテスト 1 件、`tests/windows.mjs` の段、文書 4 ページ） |
| 依存 | – |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。W01〜W37 の起票中に見つけ、利用者の依頼「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」に当たるので起票。HEAD `fa40eede` で確認） |
| 承認 | 不要（意味を変えない。COFF の弱い定義を、ELF・Mach-O と同じ「リンカーが 1 つを残す」形で出すだけ。言語の意味、ヘッダー、Windows 以外と WebAssembly の IR は変えない。ただし、Windows の target の native の IR と object は、新しい機能を使わないプログラムでも、`comdat` の属性と `$<name> = comdat any` の行の分だけすべて変わる（どの Windows の module にも `wasm.ll` の弱い定義が入るため。W チケットの共通の約束「新しい機能を使わないプログラムの IR は 1 byte も変えない」の例外で、範囲は仕様 4 と S3 が限る）。台帳は [GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30)） |
| 手本にする既存実装 | `src/runtime/io.c`・`net.c`・`gpu.c` の `__attribute__((selectany))`（COFF に弱いデータがないので、複数の翻訳単位が共有する定義を selectany にしている）、`src/llvm.rs` の `windows_abi`（Windows の target だけの IR の書き換え）、`tests/windows.mjs` の object ABI の段（`answer.obj` と C のホスト） |
| 主な影響ファイル | `src/llvm.rs`（`windows_abi`）、`src/backend/module.rs`（条件付き。設計 3）、`tests/windows.rs`、`tests/windows.mjs`、`_tsuzuri/language-reference/compiler/native-interop.md`、`_tsuzuri/language-reference/compiler/platforms.md`、`docs/architecture.md`、`docs/language.md` |

## 実装者向け早見

- 範囲: Windows の target（x86-64・Arm64、MSVC・MinGW）の native の IR。`llvm::windows_abi` の最後で、IR のすべての弱い定義（`define weak …` の関数と `@x = weak … global` の変数）に `comdat` を付け、名前ごとに `$<name> = comdat any` を module の末尾に足す。linkage は `weak` のまま変えない。
- 効果: LLVM は COFF の `weak` の定義を、既定の名前が object ごとに違う弱い外部シンボル（`.weak.<name>.default.<object の最初の公開記号>`）で書く。`comdat any` を付けると、同じ名前の普通の外部シンボルを selectany の COMDAT の節に置く。リンカーは同じ名前の COMDAT を 1 つだけ残す。
- 最初に読む: 「現状」の表 → `src/llvm.rs` の `windows_abi`（末尾の `output.push_str(include_str!("runtime/wasm.ll"))`）→ 「設計」。
- 最初に書くテスト: `tests/windows.rs` の `windows_weak_definitions_are_comdat_any`。
- 触ってはいけない: `src/runtime/wasm.ll`・`heap-small.ll`・`src/llvm_abi.rs` の `allocator` の本文（WebAssembly と POSIX が同じ文字列を使う）、`i128` の補助の呼び出し規約（[W42](_completed/W42-windows-i128-division.md) で修正済み）、`--emit header`。
- 完了の見分け方: Developer PowerShell で、「現状」の `a.obj`・`b.obj`（と `c.obj`・`d.obj`）が `lld-link` と `link.exe` の両方でリンクでき、`llvm-nm` に `.weak.` の名前がない。

## 目的

[G10](_completed/G10-windows.md) の D8 は「複数 object の weak 統合は対象外」とした。その結果、ランタイムを要しない object でも、1 つの Windows のホストには Tsuzuri の object を 1 つしか入れられない。原因は、どの Windows の object にも弱い定義が入っていることにある。弱い定義には、128 bit の補助（`__multi3` など）、所有バッファを返す object の `tsuzuri_alloc`・`tsuzuri_free`、`--allocator small` の `tz_small_shared`・`tz_small_stats` がある。`docs/language.md` は「1 つのプログラムにリンクした複数のオブジェクトは 1 つの状態を共有します」と書いており、ELF と Mach-O ではそのとおりに動く。このチケットは、COFF でも同じ名前の定義をリンカーが 1 つにまとめるようにする。

## 着手条件と停止条件

### 着手条件

- `node scripts/repo-kb.mjs brief W39`。`gh run list --branch main --workflow ci.yml --limit 1` と `windows.yml` の main の最新の結果を基準にする。
- Windows の開発機（x64）。MSVC の Developer PowerShell（PowerShell で `$vs = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022\BuildTools"; Import-Module "$vs\Common7\Tools\Microsoft.VisualStudio.DevShell.dll"; Enter-VsDevShell -VsInstallPath $vs -SkipAutomaticLocation -DevCmdArguments "-arch=x64 -host_arch=x64"`）と LLVM の clang、同梱の MinGW のツールチェーン。Arm64 は `windows.yml` と、`--target=aarch64-pc-windows-msvc` の object を `lld-link -machine:arm64` でリンクする手元の確認に頼る。
- 「現状」の再現を HEAD で繰り返し、同じ結果になることを確かめる。

### 停止条件

- **S1** 設計の書き換えの後も、`lld-link` か `link.exe`（CI の LLVM 21.1.8 か手元の clang）で、「現状」の組のどれかがリンクできない。
- **S2** Windows 以外の target・WebAssembly の IR、どの target のヘッダーが、1 byte でも変わる。
- **S3** Windows の IR の差が、`comdat` の属性と `$<name> = comdat any` の行の外に及ぶ（行末の CR は比べない。`core.autocrlf=true` の checkout でビルドしたコンパイラは `wasm.ll` を CRLF のまま足すが、書き換えは `str::lines` で読んで LF で書くので、その差は出てよい。CI は LF の checkout（G10 D7））。
- **S4** 弱い定義に、引用符付きの名前（`@"…"`）、` {` で終わらない `define weak` の行、`, align` の後に別の属性を持つ弱い変数が見つかる（設計の規則が足りない）。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

### コード

- `src/llvm.rs` の `windows_abi` は、Windows の native の IR（`--emit header` と kernel を除く。driver の `build_complete`、`test_runner.rs` の 2 か所、`emit_with_export_style`）の最後に `src/runtime/wasm.ll` を足す。中身は `define weak hidden` の `__multi3`・`__udivti3`・`__umodti3`・`__udivmodti4`・`__divti3`・`__modti3`・`__ashlti3`・`__lshrti3`・`__ashrti3` で、MSVC に compiler-rt がないことへの対処である（G10 の「実行で見つかったもの」8）。どの Windows の object にも、この 9 つが入る。
- `src/llvm_abi.rs` の `allocator()` は `define weak ptr @tsuzuri_alloc(…) nounwind {` と `define weak void @tsuzuri_free(…) nounwind {` を出す。Windows では `src/llvm.rs` の割り当ての選択が常にこちらを使う（`native_allocator` の `@tsuzuri_trap_hooks = weak hidden global` は Windows 以外だけ）。
- `--allocator small` は `src/runtime/heap-small.ll` を足す。中身は `@tz_small_shared = weak hidden global ptr @tz_small_own, align 8` と `define weak hidden void @tz_small_stats(…)` で、object ごとの `tz_small_own` のどれか 1 つをリンカーが選ぶ設計である。
- 数値の補助 `tz_soft_*` を参照する module は、ランタイムの object を使わないとき（`instrumentation.runtime_objects` が偽。`--emit object` は常にこちら）、`src/llvm.rs` が `src/runtime/numeric.ll` を IR に足す。中身は `define weak hidden` の `tz_soft_op`・`tz_soft_fma`・`tz_soft_cmp`・`tz_soft_hash_canonical`・`tz_soft_math_unary`・`tz_soft_math_binary`・`tz_soft_cast`・`tz_soft_format`・`tz_soft_format_spec`・`tz_soft_parse` の 10 個で、どれも ` {` で終わる行である。`native_runtime` には入らないので Windows の object でも `E2002` にならず、同じ衝突を起こすはずである（コードを読んだだけで、リンクは試していない。未確認）。D2 の一律の書き換えはこれも覆う。
- ランタイム入りの COFF の object は、今も `E2002`（`src/driver.rs` の `platform.windows() && native_runtime && options.emit == Emit::Object`）。このチケットは、ランタイムを要しない object（と `--emit llvm` を利用者がコンパイルしたもの）を対象にする。
- 利用者の文書（`native-interop.md`・`platforms.md`・`option.md`）には、Windows で object を 2 つ入れられないという記述がない（3 ページを「弱い」「weak」「複数」で検索して確かめた）。一方 `native-interop.md` の「言語間 LTO と bitcode」と `docs/language.md` の bitcode の節は、`tsuzuri_alloc`・`tsuzuri_free`・`tz_small_*` などを弱い定義として挙げている。

### 再現

Developer PowerShell（MSVC 14.44、LLVM 22.1.8 の clang・lld-link）で、作業は `target/scratch/w-w39w41/` で行い、確認後に消した。

```powershell
$env:TSUZURI_CLANG = "C:\Program Files\LLVM\bin\clang.exe"
# a/Main.tz: export def answer :: i64        b/Main.tz: export def twice :: i64 -> i64
#            fn answer = 42                             fn twice x = x * 2
# c/Main.tz: export def make_tag :: i64 -> [ubyte]
#            fn make_tag n = Array.init n (\i -> (65 + i) as ubyte)    （d は make_pad、97 から）
target\debug\tsuzuri.exe build a --emit object -o a.obj      # b・c・d も同じ
cl /nologo /c host.c                                          # return tz_twice(tz_answer()) == 84 ? 0 : 1;
lld-link /nologo host.obj a.obj b.obj libcmt.lib /out:ab.exe
link /nologo host.obj a.obj b.obj /out:ab.exe
```

| 組 | 結果 |
| --- | --- |
| `llvm-nm a.obj` | `W __multi3` など 9 つと、実体の `T .weak.__multi3.default.tz_answer` など。`c.obj` は加えて `W tsuzuri_alloc`・`W tsuzuri_free`（実体は `.weak.tsuzuri_alloc.default.tz_make_tag`） |
| `a.obj` + `b.obj`、lld-link | `duplicate symbol: __multi3` ほか 9 件 |
| 同じ組、link.exe | `LNK1227: conflicting weak extern definition for '__multi3'. New default '.weak.__multi3.default.tz_twice' conflicts with old default '.weak.__multi3.default.tz_answer'` |
| 同じ組、`clang --target=x86_64-pc-windows-msvc host.c a.obj b.obj` | 失敗（この機械の clang 22.1.8 のドライバーは既定で lld-link を呼んだ。`-fuse-ld=link` なら `LNK1227`） |
| `c.obj` + `d.obj`（所有バッファ） | lld-link は `tsuzuri_alloc`・`tsuzuri_free` と 9 つの補助の `duplicate symbol`、link.exe は `tsuzuri_alloc` の `LNK1227` |
| `c`・`d` を `--allocator small` で | 加えて `tz_small_shared`・`tz_small_stats` の `duplicate symbol` |
| 同梱の MinGW（`tsuzuri-clang`）で作った `a.o`・`b.o`、`c.o`・`d.o` | 同じ `.weak.` の名前を持つが、リンクでき、`c`+`d` のホストは `ABCab` を出した（MinGW の LLD は弱い外部シンボルの重複を受ける） |

### 試作（`--emit llvm` の出力を書き換えて clang `-O0`・`-O3` でコンパイルし、上の 3 組を両方のリンカーでリンク）

| 書き換え | `-O0` | `-O3` |
| --- | --- | --- |
| `linkonce_odr` + `comdat any` | 3 組とも両方で通る | `c`+`d` は `undefined symbol: tsuzuri_free`（ホストだけが参照する定義を最適化が消した）。`i128` の除算を使う組は `undefined symbol: __divti3`（呼び出しは IR の最適化の後に backend が作るので、定義が先に消えた） |
| `weak_odr` + `comdat any` | 3 組とも両方で通る | 3 組とも両方で通る |
| `weak` のまま `comdat any` を足す（設計） | 3 組とも両方で通る（`ABCab`） | 3 組とも両方で通る。`--allocator small` の組も両方で `ABCab` |

- 設計の書き換えの object は `.weak.` の名前を持たず、`tsuzuri_alloc` などは `T`、節の選択は `Any (0x2)`（`llvm-readobj --symbols`）。`$<name> = comdat any` を module の末尾に置いても、clang 22.1.8 と同梱の clang 21.1.8 の両方でコンパイルできた。
- 同梱の MinGW でコンパイルしてリンクしても、`-O0`・`-O3` とも `ABCab`。
- `--target=aarch64-pc-windows-msvc` の `c`・`d` を `lld-link -machine:arm64 -dll -noentry -force:unresolved` でリンクすると、今の IR は `duplicate symbol: tsuzuri_alloc` など、設計の IR は重複なし（CRT なしの手元の確認）。
- 新しい object 1 つと今の object 1 つは、両方のリンカーでリンクできた。今の object が 2 つ以上あれば、今と同じく失敗する。
- DLL（[W05](W05-windows-shared-library.md)・[W03](W03-windows-interop-doc-fixes.md) R3）: 設計の `c.obj` を `/DLL /DEF:`（`tsuzuri_alloc`・`tsuzuri_free`・`tz_make_tag`）でリンクすると、link.exe は 3 つとも export し、lld-link と link.exe の import library は `__imp_tsuzuri_alloc` など正しい名前になった。今の `c.obj` では link.exe が `LNK2001: unresolved external symbol tsuzuri_alloc`、lld-link の import library は `__imp_.weak.tsuzuri_alloc.default.tz_make_tag`（W03・W05 の測定と同じ）。`weak` を `dllexport` に変えて `comdat` を残した定義も、両方のリンカーで正しく export された。

## 変えてはいけない意味

- 弱い定義の意味（同じ名前の定義が複数あればリンカーが 1 つを選び、ホストや別の object の強い定義があればそちらが勝つ）。最適化は今と同じく、弱い定義の本文を呼び出し側へ取り込まない（linkage は `weak` のまま）。
- `i128` の補助・`tsuzuri_alloc`・`tsuzuri_free`・`tz_small_*` の本文と型、`dllexport` の付け方、ヘッダー。
- Windows 以外の target と WebAssembly の IR。

## 仕様

1. Windows の target の native の IR では、`windows_abi` が返すすべての弱い定義が `comdat any` の COMDAT に入る。COMDAT の名前は定義の名前と同じである。
2. linkage・可視性・属性・本文は変えない。
3. 2 つ以上の Tsuzuri の object（ランタイムを要しないもの）を、lld-link・link.exe・同梱の MinGW のどれでも 1 つのホストにリンクできる。同じ名前の弱い定義は 1 つが残る（ELF・Mach-O と同じ）。どれが残るかは決めない。`--allocator` を揃える責任は今と同じく利用者にある（`docs/language.md` の「1 つのプログラムにリンクした複数のオブジェクト」）。
4. Windows 以外の target・WebAssembly の IR とすべてのヘッダーは変わらない。Windows の IR の差は、`comdat` の属性と module の末尾の `$<name> = comdat any` の行だけである。

## 設計

### 1. `src/llvm.rs`

```rust
/// W39: LLVM writes a `weak` definition for COFF as a weak external whose default is named after the
/// object (`.weak.<name>.default.<first symbol>`), and lld-link and link.exe reject a second one. The
/// same definition in a `comdat any` (selectany) section is kept once, as ELF and Mach-O keep one weak
/// definition. The linkage stays `weak`: `linkonce_odr` lets the optimizer drop what only the host or a
/// libcall reaches (`tsuzuri_free`, `__divti3`), and `weak_odr` would call different bodies equal.
fn with_coff_comdats(ir: &str) -> String
```

- `windows_abi` の最後（`wasm.ll` を足した後）で `output = with_coff_comdats(&output)` とする。`windows_abi` を通るすべての経路（driver、`tsuzuri test` の 2 か所、`emit_with_export_style`）に効く。
- 行は `str::lines` で読む（CRLF の checkout でビルドしたコンパイラは `include_str!` の `wasm.ll` を CRLF のまま埋め込むので、`split('\n')` で読むと行末に `\r` が残る）。
- `define weak ` で始まる行: 名前は最初の `@` から `(` まで。行末の ` {` の前（` !` で始まる metadata の付加があればその前）に ` comdat` を入れる。
- `@<name> = weak ` で始まる行: `, align N` の前に `, comdat` を入れる。`, align` がなければ行末に足す。
- 名前を最初に現れた順に覚え、module の末尾に `$<name> = comdat any` を 1 行ずつ足す（決定的）。同じ名前は 1 回だけ。
- 名前を列挙しない（D2）。`tsuzuri_trap_hooks` のように Windows では今出ない弱い定義も、将来出れば同じ扱いになる。

### 2. 既存の書き換えとの関係

- `windows_abi` が `dllexport` を付けるのは `tz_*` と `tsuzuri_main` の強い定義だけなので、この書き換えとは重ならない。
- codegen units（`src/driver/units.rs` の `eligible`）は Windows の native で使わない。`llvm_split` は `comdat` の行を受けないが、届かない。
- `with_codeview` は module flags だけを変えるので、順序の制約はない。

### 3. `--backend fast`（条件付き）

`src/backend/module.rs` の関数の属性の解析は `comdat` を `bail!("the function attribute 'comdat'")` で拒む（大域変数の `comdat` と `$` の行は読み飛ばす）。fast backend の可否は `src/driver.rs` の `BuildOptions::validate_fast` がホストの OS（`cfg!(target_os)`）で決めるので、Windows のホストでは届かない（`--backend fast is not available on windows-x86_64`）。macOS・Linux のホストで `--target x86_64-pc-windows-msvc --backend fast --emit object` が今通るなら、関数の `comdat` も大域変数と同じく読み飛ばす（出力は今と同じになる）。通らないなら変えない（W03 の (6) と同じ問い）。

## 前提とする他チケットのインターフェース

- なし。

## 他チケットへ提供するインターフェース

- Windows の IR の弱い定義の行の形: `define weak ptr @tsuzuri_alloc(i64 %size) nounwind comdat {`、`define weak void @tsuzuri_free(ptr %value) nounwind comdat {`、末尾に `$tsuzuri_alloc = comdat any`。
- [W05](W05-windows-shared-library.md): D2 の書き換え（2 行の完全一致で `weak` を `dllexport` にする）は、このチケットの後では一致しなくなる。行頭の `define weak ` を `define dllexport ` に変え、`comdat` は残してよい（両方のリンカーで export と import library が正しいことを確かめた）。このチケットの後なら、`.def` で弱い定義を export する案も成り立つ（link.exe が export できた）。どちらにするかは W05 が決める。
- [W03](W03-windows-interop-doc-fixes.md): R3 の回避策の文（link.exe は `tsuzuri_alloc`・`tsuzuri_free` を export できない、import library は `.def` から作り直す）は、このチケットの後では正しくない。後に入る方が直す。
- [W35](W35-windows-trap-mode-return.md): D4 の理由（2 つの object の `__multi3` が衝突する）と、テストの `-DMIXED_OBJECTS` を Windows で飛ばす理由は、このチケットの後では成り立たない。trap ランタイムの強い定義（`TZ_TRAP_API` が空）の重複は残るので、W35 の結論が変わるかは W35 が確かめる。

## 実装手順

### 手順 1: テスト

- 変更: `tests/windows.rs`。
- 内容: テスト計画 1。今の HEAD で失敗することを確かめる。
- 確認: `cargo test --locked --test windows`（`running N tests` を見る）。

### 手順 2: 書き換え

- 変更: `src/llvm.rs`、条件付きで `src/backend/module.rs`（設計 3）。
- 内容: 設計 1〜3。
- 確認: T0。`cargo nextest run --locked`。`tests/fixtures`・`examples` の `--emit llvm` を、Windows 以外の target（`--target x86_64-unknown-linux-gnu`・`aarch64-apple-darwin`）と wasm32 で変更の前後に比べ、byte 単位で同じ（S2）。Windows の target では差が `comdat` の行と属性だけ（S3）。

### 手順 3: E2E と文書

- 変更: `tests/windows.mjs`、「ドキュメント」のページ。
- 内容: テスト計画 2。
- 確認: `cargo build && node tests/windows.mjs target/debug/tsuzuri.exe`（Developer PowerShell）。同梱の MinGW で「現状」の組を手で確かめる。`node scripts/check-docs.mjs <変えたページ>`。T2。PR に `ci:full` を付け、`windows.yml`（x64・Arm64）が通る。

## テスト計画

1. **IR**（`tests/windows.rs` の `windows_weak_definitions_are_comdat_any`、すべてのホスト）: 所有バッファを返す export を持つ module を `--allocator small` と既定で `emit_with_export_style(…, Dllexport)` に通す。`tz_soft_*` を参照して `numeric.ll` を足す module（どの式が参照するかは着手時に `--emit llvm` で選ぶ）も通す。すべての `define weak`・`= weak` の行が `comdat` を持ち、その名前ごとに `$<name> = comdat any` がちょうど 1 行あり、2 回の出力が同じ。既存の `emits_windows_exports_without_changing_default_or_wasm` の既定と wasm32 の IR に `comdat` がないことも確かめる。
2. **E2E**（`tests/windows.mjs` の object ABI の段の後）: `answer`・`twice`、`make_tag`・`make_pad`（既定と `--allocator small`）の object の組（`-O0` と `-O3` のそれぞれ。`linkonce_odr` の失敗は `-O3` だけに出た）を、それぞれ 1 つの C のホストに、`nativeFlags`（`-fuse-ld=lld`）と `-fuse-ld=link`（MSVC の link.exe）でリンクして実行し、結果（`84`、`ABCab`）を確かめる。`llvm-nm` に `.weak.` の名前がないことも見る。[W42](_completed/W42-windows-i128-division.md) が入ったので、`i128` の除算の結果も確かめる。
3. **既存**: `tests/windows.mjs` の全段、`tests/fast_backend.rs`、`tests/trap_locations.rs`（どちらも `wasm.ll` を自分で足す）、`tests/host_abi.rs`。

## ドキュメント

- `_tsuzuri/language-reference/compiler/native-interop.md`: 「ヘッダーとオブジェクト」の節に「ランタイムを要しない object は、Windows を含むすべての target で、いくつでも 1 つのホストにリンクできる。同じ名前の弱い定義（`tsuzuri_alloc` など）はリンカーが 1 つを残す。`--allocator` は揃える」と書く。「言語間 LTO と bitcode」の弱い定義の列挙は変えない。
- `_tsuzuri/language-reference/compiler/platforms.md`: Windows の項に、COFF では弱い定義を selectany の COMDAT で出すこと。
- `docs/architecture.md`: `windows_abi` と `wasm.ll` を説明する箇所に、COFF の `comdat any` の理由（既定の名前の衝突、`linkonce_odr` を採らない理由）を 1 段落。
- `docs/language.md`: 「1 つのプログラムにリンクした複数のオブジェクト」の段落に、Windows でも同じであることを足す。W35 が先に入って「Windows の 1 つのホストに Tsuzuri の object は 1 つ」の文（W35 D4）があれば消す。

## 受け入れ条件

- [ ] 「仕様」1〜4 のとおりに動き、「現状」の組（既定と `--allocator small`）が lld-link・link.exe・同梱の MinGW でリンクでき、実行結果が正しい。
- [ ] Windows 以外の target・wasm32 の IR とすべてのヘッダーが変更の前後で同じ。Windows の IR の差は `comdat` の属性と `$<name> = comdat any` の行だけ。
- [ ] `tests/windows.rs` と `tests/windows.mjs` の新しい検査が通る。T2 が通る。
- [ ] 文書を直し、`check-docs` が通る。PR に `ci:full` を付け、`windows.yml` が通る。

## 落とし穴

- **`linkonce_odr` にしない**: `-O3` で、ホストだけが参照する `tsuzuri_alloc`・`tsuzuri_free` と、backend が IR の最適化の後に呼び出しを作る `__divti3` などが消える（試作で確認）。
- `weak_odr` にしない: 本文が同じだと最適化に約束することになる。`tsuzuri_alloc` は `--allocator` ごとに違い、`tz_small_shared` の初期値は object ごとに違う。
- `--allocator` の違う object を混ぜても、COFF でもリンクは通るようになる（今は通らないので気づける）。ELF と同じ契約で、検出はしない（D5）。文書の「`--allocator` は揃えてください」を Windows の読者にも届く位置に置く。
- `$<name> = comdat any` を `source_filename` より前に置くと、clang が `expected top-level entity` で拒む（試作で確認）。module の末尾に置く。
- 行末の ` {` だけを見て ` comdat` を入れると、metadata の付加（` !dbg !N`）を持つ行で順序が LLVM の文法に反する。今の弱い定義は付加を持たないが、規則は文法どおりにする。

## 対象外

- **Windows x86-64 の `i128` の除算の呼び出し規約**: [W42](_completed/W42-windows-i128-division.md) で Win64 の C の ABI の包み `src/runtime/i128-windows-x64.ll` に修正済み。このチケットは補助の包みの linkage（`comdat`）だけを変え、本文と規約は変えない。
- ランタイムを含む COFF の object の `E2002`（G10 D8 の前半）と、trap ランタイムの強い定義（[W35](W35-windows-trap-mode-return.md)）。
- DLL の出力（[W05](W05-windows-shared-library.md)）。
- 今の版のコンパイラで作った object どうしの組（2 つ以上あれば今と同じく失敗する）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | Windows の target では、弱い定義の linkage を `weak` のまま、`comdat any` を足す | 試作で 3 組（既定と `--allocator small`）が両方のリンカー・同梱の MinGW・`-O0`/`-O3` で通り、Arm64 の lld-link でも重複がなかった。最適化の扱いは今と同じ。退けた案: `linkonce_odr`（`-O3` で `tsuzuri_free`・`__divti3` が消える。測定）、`weak_odr`（通るが、本文が同じという約束が `tsuzuri_alloc`・`tz_small_shared` で偽になる）、文書に「1 つのホストに 1 つ」と書くだけ（`docs/language.md` の複数の object の契約が Windows だけ成り立たない） | 既定案（実装者はこの案に従う） |
| D2 | 名前を列挙せず、`windows_abi` の最後で IR のすべての弱い定義に一律に適用する | `wasm.ll` の補助・`allocator`・`heap-small.ll` の 3 か所を個別に直すと、WebAssembly と POSIX が同じ文字列を使うので分岐が増える。Windows の IR は 1 か所（`windows_abi`）を必ず通る | 既定案（実装者はこの案に従う） |
| D3 | `$<name> = comdat any` は module の末尾に、最初に現れた順で置く | 先頭の `source_filename` より前には置けない（測定）。末尾の前方参照は clang 21.1.8・22.1.8 で通った。順序を固定して IR を決定的にする | 既定案（実装者はこの案に従う） |
| D4 | MSVC と MinGW を分けない | 同梱の MinGW は今も通るが、書き換えの後も通った（測定）。target の種類で IR を分けない方が単純 | 既定案（実装者はこの案に従う） |
| D5 | `--allocator` の違う object の組を検出しない | ELF・Mach-O と同じ契約（`docs/language.md`）。リンカーの選択は object の外にあり、コンパイラからは見えない | 既定案（実装者はこの案に従う） |
| D6 | `--backend fast` が Windows の target の IR を読むなら、関数の `comdat` を大域変数と同じく読み飛ばす | 読み飛ばさないと、この書き換えで fast backend の Windows の出力が `E2002` になる。届くかは設計 3 で確かめる | 既定案（実装者はこの案に従う） |
