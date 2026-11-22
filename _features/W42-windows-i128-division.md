# W42: Windows x64 の `i128`・`i128u` の `/`・`%` が誤った値になり、リンクしたホストの `__int128` の除算も壊す

| 項目 | 内容 |
| --- | --- |
| ID | W42 |
| 優先度 | P1（トラップも診断もなく、黙って誤った値を返す。`-O3` で正しく見えるのは、定数のオペランドを畳み込んだときだけである。実行時の値なら `-O3` でも誤る。さらに、Tsuzuri の object をリンクしたホストでは、ホスト自身の C の `__int128` の除算まで誤る） |
| 規模 | M（新しい `src/runtime/i128-windows-x64.ll`（9 つの包み）、`src/llvm.rs` の 1 つの関数と `windows_abi` の末尾、Rust のテスト 2 件、`tests/windows.mjs` の 2 段、`tests/integer_format.mjs`・`tests/trap_locations.rs`・`src/abi.rs` のテストの追従、文書 2 ページと注釈） |
| 依存 | – |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。W01〜W41 の起票中に見つけ、利用者の依頼「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」に当たるので起票。HEAD `fa40eede` で確認） |
| 承認 | 不要（意味を正しくする修正。言語の意味、ヘッダー、Windows Arm64・Windows 以外・WebAssembly の IR は変えない。ただし Windows x86-64 の native の IR は、新しい機能を使わないプログラムでも、末尾に足す 128 bit の補助の部分だけがすべて変わる。どの Windows の module にも補助が入るためで、W チケットの共通の約束「新しい機能を使わないプログラムの IR は 1 byte も変えない」（[README の「共通の綴り」](README.md#共通の綴り起票時に決めた契約)）の例外になる。範囲は仕様 4 と S3 が限る。台帳は [GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30)） |
| 手本にする既存実装 | `src/runtime/wasm.ll` の補助（本体をそのまま使う）、`src/llvm.rs` の `windows_abi`（Windows の target だけの IR の書き換え）、`crate::target::current_platform()` で target を読む既存の分岐（`src/llvm.rs` の `emit_program` の `with_stack_probes` の呼び出し）、`tests/e2e.mjs` の `runtimeChecks`（BigInt の参照と 128 bit の組） |
| 主な影響ファイル | `src/runtime/i128-windows-x64.ll`（新規）、`src/llvm.rs`（`windows_abi`、新しい `windows_i128_helpers`）、`src/abi.rs`（テストの一覧）、`tests/windows.rs`、`tests/windows.mjs`、`tests/integer_format.mjs`、`tests/trap_locations.rs`、`tests/prebuilt_runtime.rs`（注釈）、`src/runtime/integer.ll`（注釈）、`_tsuzuri/language-reference/compiler/platforms.md`、`docs/architecture.md` |

## 実装者向け早見

- 範囲: Windows x86-64（MSVC・MinGW）の native の IR の末尾に足す 128 bit の補助だけを変える。Windows Arm64、ほかの target、WebAssembly は変えない。
- 原因: LLVM は x86-64 の Windows で `sdiv`・`udiv`・`srem`・`urem i128` を `__divti3` などの呼び出しに下げる。オペランドは呼び出し側の一時領域へのポインター（RCX・RDX）で渡し、結果は XMM0 で受ける。ところが `windows_abi` が足す `wasm.ll` の定義は IR の `i128` の引数なので、RCX:RDX と R8:R9 で受けて RAX:RDX で返す。呼び出し側は、XMM0 に残っていた値を結果として読む。
- 直し方: x86-64 の Windows では、`wasm.ll` の 9 行の `define weak hidden i128 @__<name>(` を `define internal i128 @tz.runtime.<name>(` に読み替えて足す。そのうえで、新しい `src/runtime/i128-windows-x64.ll` の弱い包みを 9 つ足す。包みは Win64 の C の ABI の形（`i128` はポインターで受け、`<2 x i64>` を返す）で書く。x86-64 か Arm64 かは `crate::target::current_platform().aarch64()` で選ぶ。
- 最初に読む: 「現状」の表 → `src/llvm.rs` の `windows_abi` の最後の `output.push_str(include_str!("runtime/wasm.ll"))` → 「設計」。
- 最初に書くテスト: `tests/windows.rs` の `windows_x64_i128_helpers_follow_the_win64_abi`。
- 触ってはいけない: `src/runtime/wasm.ll`（WebAssembly と Windows Arm64 がそのまま使う）、`integer.ll` の除算を避ける実装、補助の linkage（`weak` のまま）。
- 完了の見分け方: 「現状」の 4 つの再現が、MSVC（link.exe・lld-link）と同梱の MinGW の `-O0`・`-O3` で正しい値を出す。

## 目的

Windows x64 では、`i128`・`i128u` の `/` と `%` が黙って誤った値を返す。言語リファレンスはこれを既知の制限として書いている（`platforms.md` の「既知の制限」）。補助を、LLVM と C コンパイラが Windows x64 で使う呼び出し規約で定義し直す。そうすれば、どの最適化の水準、どのリンカー、どのホストでも正しい値になる。そのうえで既知の制限を消す。

## 着手条件と停止条件

### 着手条件

- `node scripts/repo-kb.mjs brief W42`。`gh run list --branch main --workflow ci.yml --limit 1` と `windows.yml` の main の最新の結果を基準にする。
- Windows x64 の開発機。MSVC の Developer PowerShell（`$vs = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022\BuildTools"; Import-Module "$vs\Common7\Tools\Microsoft.VisualStudio.DevShell.dll"; Enter-VsDevShell -VsInstallPath $vs -SkipAutomaticLocation -DevCmdArguments "-arch=x64 -host_arch=x64"` の後に `$env:TSUZURI_CLANG = "C:\Program Files\LLVM\bin\clang.exe"`）と、同梱の MinGW（Git Bash で `T=$HOME/.vscode/extensions/tmidorikawa.tsuzuri-0.1.6-win32-x64/toolchain`、`TSUZURI_CLANG=$T/bin/tsuzuri-clang.exe`）。Arm64 の実行は `windows.yml` の Arm64 の job に頼る。
- 「現状」の再現を HEAD で繰り返し、同じ結果になることを確かめる。

### 停止条件

- **S1** 設計の補助にしても、「現状」の再現のどれかが MSVC（link.exe・lld-link）か同梱の MinGW の `-O0`・`-O3` で誤る。
- **S2** Windows Arm64・Windows 以外の target・WebAssembly の IR、どの target のヘッダーが、1 byte でも変わる。
- **S3** Windows x86-64 の IR の差が、末尾の補助の部分の外に及ぶ。補助の部分とは、`wasm.ll` の 9 行の読み替えと足した包みのことである。
- **S4** CI の LLVM（21.1.8）か手元の clang で、x86-64 の Windows の libcall の形が「現状」と違う（ポインターで渡さない、XMM0 で返さない）。
- **S5** Windows で `tests/integer_format.mjs` の `numeric.ll` との照合を有効にすると、`i128` の除算と関係のない理由で一致しない。その場合、照合は今のまま飛ばし、報告する。
- **S6** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

### コード

- `src/llvm.rs` の `windows_abi` は、Windows の native の IR の最後に `src/runtime/wasm.ll` を足す（218 行目）。呼び出し元は、driver の `build_complete`（4119 行目）、`src/test_runner.rs` の 2 か所（489・973 行目）、`emit_with_export_style` の 4 つである。driver と test runner の呼び出しでは、どれも `PlatformScope` が生きている（`build_complete` の 3792 行目、`test_runner.rs` の `build_debug_runner`・`build_runner`・`run_benches_linked` の 474・720・1264 行目。`test_runner.rs` の単体テスト `bench_runner_rejects_malformed_arguments` だけは scope なしでビルド機を使う）。`emit_with_export_style`（公開。`tests/windows.rs` が使う）は scope を作らないので、`current_platform()` はビルド機（`Platform::HOST`）を答える。
- `wasm.ll` の補助は 9 つある。どれも `define weak hidden i128 @__` で始まる行で、`__multi3`・`__udivti3`・`__umodti3`・`__udivmodti4`・`__divti3`・`__modti3`・`__ashlti3`・`__lshrti3`・`__ashrti3` である。本体は、内部の `tz.runtime.udivmod`（引き戻し法の除算）と `tz.runtime.shift` を呼ぶ。補助どうしは名前で呼び合わない。
- LLVM が libcall にする命令を調べた。`mul`・`shl`・`lshr`・`ashr`・`sdiv`・`srem`・`udiv`・`urem`・`smul`/`umul.with.overflow` の `i128` を clang 22.1.8 で `-O0`・`-O3` にコンパイルし、`llvm-nm -u` を見た。x86-64・Arm64 の Windows、x86-64 Linux、Arm64 macOS のどれでも、未定義になるのは `__divti3`・`__modti3`・`__udivti3`・`__umodti3` だけだった。定数の除数でも、`sdiv i128 %a, 3`・`srem i128 %a, 7`・`udiv i128 %a, 1000000000` は `-O3` で libcall のままだった。`udiv i128 %a, 10` だけはその場に展開された。
- 呼び出し側と定義が食い違う（`--emit llvm` の IR を `--target=x86_64-pc-windows-msvc` と `-gnu` の `-O0` でコンパイルし、`llvm-objdump -dr` で確認）。呼び出し側は `leaq` で RCX・RDX にポインターを置いて `__divti3` を呼び、結果を `movq %xmm0, %rcx` で読む。定義（`.weak.__divti3.default.<x>`）は RCX:RDX を被除数、R8:R9 を除数として読み、RAX:RDX に書いて返す。
- clang の C の `__int128` も、LLVM の libcall と同じ形である（clang 21.1.8 と 22.1.8、`x86_64-pc-windows-msvc` と `-gnu`）。`__int128 f(__int128, __int128)` は `<2 x i64> @f(ptr, ptr)` になり、引数を `load i128, ptr, align 16` で読む。剰余のポインターを取る形は `(ptr, ptr, ptr)`、シフトの形は `(ptr, i32)` である。
- Windows Arm64 は食い違わない（アセンブリの読解だけで、実行は未確認）。`aarch64-pc-windows-msvc`・`-gnu` の C の ABI は `i128 @f(i128, i128)` である。`--target aarch64-pc-windows-msvc` の IR を `-O0`・`-O3` でコンパイルすると、呼び出し側は X0:X1・X2:X3 に置き、`wasm.ll` の定義も X0〜X3 を読む。
- `numeric.ll` は、x86-64 の Windows で `-O0` にコンパイルすると `tz_soft_format_spec`（`radix_text` の可変の基数の除算）から `__udivti3` を参照する。`-O2` では参照しない。整数の書式は `integer.ll` が受け持つので、コンパイラの出力からこの経路には届かない。一方 `tests/integer_format.mjs` はこの関数を直接呼ぶため、Windows では照合を飛ばしている（`crossCheck = process.platform !== "win32"`）。
- `integer.ll` は `i128` を割らない（冒頭の注釈 12〜15 行目）。`tests/prebuilt_runtime.rs` の `integer_runtime_never_divides_an_i128` と、`tests/integer_format.mjs` のアセンブリの検査がこれを守る。どちらも理由にこの不具合を挙げている。
- `tests/trap_locations.rs` は、Windows のホストで native の IR に `wasm.ll` を自分で足す（497〜503 行目、「`windows_abi` と同じ」）。`tests/e2e.mjs` の `runtimeChecks` は補助を wasm32 でだけ検査する。Windows で `i128` の除算を実行するテストはない（`tests/windows.mjs` に `i128` はない）。
- 文書: `platforms.md` の「既知の制限」の 3 つ目の項（167 行目）、`docs/architecture.md` の `integer.ll` の行（104 行目）と `wasm.ll` の行（112 行目）。完了済みの [G10](_completed/G10-windows.md)（「実行で見つかったもの」8）と [PM08](../_perfs/_completed/PM08-binary-size.md) にも記述がある。

### 再現

作業は `target/scratch/w-w42/` で行い、確認後に消した。プログラムは 3 つ使った。`a` は下の定数のプログラム、`b` は 2 行を標準入力から `Parse.parse` で `i128` に読み、`{a / b} {a % b} {ua / ub} {ua % ub}` を出す（`ua`・`ub` は `as i128u`）。`c` は `export def sdiv :: i64 -> i64 -> i64 -> i64`（`((x as i128) * (scale as i128)) / (y as i128)`）と、`smod`・`udiv`・`umod` を持つ library で、C のホストから呼ぶ。

```tsuzuri
def main :: unit -> i32 = \() ->
    let a: i128 = 100i128
    let b: i128 = 3i128
    do! IO.writeln (to_string (a / b))
    do! IO.writeln (to_string (a % b))
    0
```

```sh
TSUZURI_CLANG=$T/bin/tsuzuri-clang.exe target/debug/tsuzuri.exe build a -O0 -o a0.exe   # -O3、b も同じ
printf '100\n3\n' | ./b0.exe
TSUZURI_CLANG=$T/bin/tsuzuri-clang.exe target/debug/tsuzuri.exe build c -O0 --emit object -o c0.o
$T/bin/tsuzuri-clang.exe host.c c0.o -o host.exe    # MSVC: --emit object -o c0.obj、clang --target=x86_64-pc-windows-msvc -c host.c（cl は __int128 を持たない）、link host.obj c0.obj
```

| 再現 | MinGW `-O0` | MinGW `-O3` | MSVC `-O0` | MSVC `-O3` |
| --- | --- | --- | --- | --- |
| `a` | `0`・`0` | `33`・`1`（定数を畳み込み、`__divti3` の呼び出しが消える） | `0`・`0` | `33`・`1` |
| `b`（`100`・`3`） | `0 0 0 0` | `0 100 0 100` | `0 0 0 0` | `0 100 0 100` |
| `b`（`-100`・`7`） | `0 0 0 0` | `0 -100 0 340282366920938463463374607431768211356` | 同じ | 同じ |
| `c` と C のホスト（10 件の検査） | 10 件とも誤り | 10 件とも誤り | 10 件とも誤り（link.exe） | 10 件とも誤り |
| ホスト自身の `__int128` の `100 / 3`（`c` をリンク） | `0` | – | `140697252487632`（`libcmt` とリンク） | – |

- 誤った値は XMM0 に残っていた値で、`0` とは限らない。`-O3` の `%` が被除数になるのは、LLVM が `a - (a / b) * b` に分けたためである。
- `c` をリンクしないと、MSVC のホストは `unresolved external symbol __divti3` になる（MSVC に compiler-rt はない）。同梱の MinGW では、ホストの参照を zig の compiler_rt ではなく object の弱い `__divti3` が満たしたとみられる（ホストの結果が `0` になり、設計の object では `33` になったことからの推定）。

### 試作（`--emit llvm` の末尾の `wasm.ll` を設計の形に置き換え、clang `-O0`・`-O3` でコンパイル）

- `c` の `-O0`・`-O3` の IR を置き換えた object は、MSVC の link.exe・lld-link と同梱の MinGW で、`c` の 10 件、ホストの `__int128` の `100 / 3`（`33`）、C から直接呼んだ `__udivmodti4`・`__multi3`・`__ashlti3`・`__ashrti3`（2 組の値）がすべて正しかった。今の `wasm.ll` の補助では、C から呼んだこれらも誤った（`__multi3` は `0`）。包みの `__divti3`（clang 22.1.8 の `-O0`）は、RCX と RDX が指す 16 byte ずつを読み、結果を `punpcklqdq` で XMM0 に組んで返す。

## 変えてはいけない意味

- 補助の本体（引き戻し法の除算、0 で割ったときの `llvm.trap`、符号の扱い、リムのシフト）と、`weak hidden` の linkage。
- `/`・`%` の前の検査（0 除算と `MIN / -1` のトラップ）は IR の側にあり、変えない。
- Windows Arm64・Windows 以外・WebAssembly の IR とヘッダー。

## 仕様

1. Windows x86-64 の native の IR では、`__multi3`・`__udivti3`・`__umodti3`・`__udivmodti4`・`__divti3`・`__modti3`・`__ashlti3`・`__lshrti3`・`__ashrti3` の弱い定義が、Win64 の C の ABI の `__int128` の形になる。`i128` の引数はポインターで受け、結果は `<2 x i64>`（XMM0）で返す。剰余のポインターとシフトの量はそのまま受ける。
2. `i128`・`i128u` の `/`・`%` は、Windows x64 でも、どの最適化の水準でも、ほかの target と同じ値になる。
3. Tsuzuri の object をリンクしたホストの `__int128` の除算・乗算・シフトの呼び出しも、正しい値になる（補助を選んだ場合）。
4. Windows Arm64・Windows 以外・WebAssembly の IR とすべてのヘッダーは変わらない。Windows x86-64 の IR の差は、末尾の補助の部分だけである。

## 設計

### 1. `src/runtime/i128-windows-x64.ll`（新規）

先頭の注釈に、Windows x64 の libcall と C の `__int128` の規約、本体が `wasm.ll` にあること、名前の読み替えを書く。包みは 9 つで、同じ形をとる。

```llvm
define weak hidden <2 x i64> @__divti3(ptr %a, ptr %b) nounwind {
entry:
  %x = load i128, ptr %a, align 16
  %y = load i128, ptr %b, align 16
  %r = call i128 @tz.runtime.divti3(i128 %x, i128 %y)
  %v = bitcast i128 %r to <2 x i64>
  ret <2 x i64> %v
}
```

`__multi3`・`__udivti3`・`__umodti3`・`__modti3` は同じ `(ptr, ptr)`、`__udivmodti4` は `(ptr %a, ptr %b, ptr %remainder)` で剰余のポインターをそのまま渡し、3 つのシフトは `(ptr %value, i32 %count)` である。

### 2. `src/llvm.rs`

```rust
/// W42: the 128-bit routines that `windows_abi` appends. x86-64 Windows calls them with each i128 operand
/// in a temporary passed by pointer and takes the result in XMM0, the C ABI of `__int128` there, so the
/// bodies of wasm.ll become internal `tz.runtime.<name>` functions behind wrappers of that shape. Arm64
/// passes an i128 in two registers, as the definitions of wasm.ll take it.
pub fn windows_i128_helpers(aarch64: bool) -> String
```

- Arm64 は `wasm.ll` をそのまま返す。x86-64 は、`wasm.ll` の `define weak hidden i128 @__` を `define internal i128 @tz.runtime.` に置き換え、`i128-windows-x64.ll` を続ける。今の内部の名前（`tz.runtime.udivmod`・`tz.runtime.shift`）とは衝突しない。
- `windows_abi` の最後を `output.push_str(&windows_i128_helpers(crate::target::current_platform().aarch64()))` にする。呼び出し元の署名は変えない。
- 公開するのは、`tests/trap_locations.rs` が同じ文字列を足せるようにするためである。

## 前提とする他チケットのインターフェース

- なし。

## 他チケットへ提供するインターフェース

- [W39](W39-windows-multiple-objects-weak-defaults.md): このチケットの後、Windows x86-64 の IR の弱い定義は 9 つの包み（`define weak hidden <2 x i64> @__divti3(ptr %a, ptr %b) nounwind {` など）になり、本体は `internal` になる。W39 の名前を列挙しない書き換え（D2）がそのまま包みに当たり、本体には `comdat` が要らない。マージはどちらの順でもよい。W39 の「対象外」の最初の項と、テスト計画 2 の「`i128` の除算の結果は確かめない」は、後に入る方が直す（W39 の E2E に除算の結果を足せる）。
- `tsuzuri::llvm::windows_i128_helpers(aarch64)`: Windows の補助を自分で足すテストのための入口。

## 実装手順

### 手順 1: テスト

- 変更: `tests/windows.rs`、`tests/windows.mjs`。
- 内容: テスト計画 1〜3。今の HEAD の x64 で 3 つとも失敗することを確かめる。
- 確認: `cargo test --locked --test windows`（`running N tests` を見る）。Developer PowerShell で `cargo build && node tests/windows.mjs target/debug/tsuzuri.exe`。

### 手順 2: 補助

- 変更: `src/runtime/i128-windows-x64.ll`、`src/llvm.rs`。
- 内容: 設計 1・2。
- 確認: T0。`cargo nextest run --locked`。`tests/fixtures`・`examples` の `--emit llvm` を、`--target aarch64-pc-windows-msvc`・`x86_64-unknown-linux-gnu`・`aarch64-apple-darwin` と wasm32 で変更の前後に比べ、byte 単位で同じ（S2）。`x86_64-pc-windows-msvc` では差が末尾の補助の部分だけ（S3）。

### 手順 3: 追従

- 変更: `src/abi.rs`、`tests/trap_locations.rs`、`tests/integer_format.mjs`、`tests/prebuilt_runtime.rs`、`src/runtime/integer.ll`。
- 内容: `reserved_symbols_cover_the_runtime_names` の一覧に新しいファイルを足す。`trap_locations.rs` は `windows_i128_helpers(cfg!(target_arch = "aarch64"))` を足す。`integer_format.mjs` は Windows でも `crossCheck` を有効にし、x64 では `wasm.ll` の同じ置き換えと新しいファイルを、Arm64 では `wasm.ll` を連結する（置き換えが `windows_i128_helpers` と同じであることを注釈に書く）。`integer.ll` を割らない理由の注釈を D6 のとおり書き直す。
- 確認: `node tests/integer_format.mjs`（Windows と Linux）。`cargo test --locked --test trap_locations --test prebuilt_runtime`。S5 に当たれば止める。

### 手順 4: 文書と CI

- 変更: 「ドキュメント」のページ。
- 内容: 「ドキュメント」のとおり。
- 確認: `node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/platforms.md`。T2。PR に `ci:full` を付け、`windows.yml`（x64・Arm64）が通る。

## テスト計画

1. **IR**（`tests/windows.rs` の `windows_x64_i128_helpers_follow_the_win64_abi`、すべてのホスト）: `PlatformScope::enter(Platform::new(Triple::parse("x86_64-pc-windows-msvc")))`（と `-gnu`）の中で `emit_with_export_style(…, Dllexport)` を呼ぶ。IR に 9 つの `define weak hidden <2 x i64> @__…(ptr` があり、`define weak hidden i128 @__` がなく、`define internal i128 @tz.runtime.` が 9 つあり、2 回の出力が同じであることを確かめる。`aarch64-pc-windows-msvc` の scope では、末尾が `wasm.ll` と byte 単位で同じであることを確かめる。加えて、`wasm.ll` と `i128-windows-x64.ll` の弱い定義の名前の集合が同じであることを確かめる（`wasm.ll` に補助を足したら包みも要る）。
2. **参照**（`tests/windows.mjs`、x64 と Arm64 のホスト）: `answer` の library を `-O0`・`-O3` の `--emit object` にし、`__int128` の `/`・`%`・`*`・`<<`・`>>` と、`__udivmodti4` の直接の呼び出し（剰余のポインターありとなし）を行う C のハーネスとリンクする。ハーネスは組を標準入力から読む。`tests/e2e.mjs` の `runtimeChecks` と同じ 7 つの端の組と 100 の擬似乱数の組に負の値を足し、node の BigInt と比べる。符号付きの `MIN / -1` と 0 除算は除く。C の参照の計算に符号付きのシフトを使わない（同梱の clang は `-O0` で UBSan を入れる。[W37](W37-bundled-clang-ubsan-objects.md)）。
3. **E2E**（`tests/windows.mjs`）: 「現状」の `b` を `-O0`・`-O3` で `run` して標準入力を渡す（`execute` に入力の引数を足す）。`c` とその C のホストも同じく確かめる。期待値は `33 1 33 1`、`-14 -2 …` のように正確に書く（誤った値は `0` とは限らない）。
4. **既存**: `tests/e2e.mjs`（wasm32 の補助）、`tests/windows.mjs` の全段、`tests/trap_locations.rs`、`tests/integer_format.mjs`、`tests/prebuilt_runtime.rs`。

## ドキュメント

- `_tsuzuri/language-reference/compiler/platforms.md`: 「既知の制限」の `i128` の項を消す。「確かめた範囲」の `windows.yml` の項に「`i128` の除算と剰余」を足す。
- `docs/architecture.md`: `integer.ll` の行の、Windows で桁が壊れるという文を、「`i128` を割らず、除算の補助を呼ばない（D6）」と、W42 までの経緯の 1 文に直す。`wasm.ll` の行に「Windows x86-64 は本体を内部の関数に読み替えて足し、`i128-windows-x64.ll` の包みを添える」と書き、新しいファイルの行を足す。
- `docs/language.md`・README は、この不具合を書いていないので変えない（「XMM0」「udivti3」で検索して確かめた）。完了済みの G10・PM08 は記録なので直さない。

## 受け入れ条件

- [ ] 「現状」の 4 つの再現が、MSVC（link.exe・lld-link）と同梱の MinGW の `-O0`・`-O3` で正しい値を出す。
- [ ] Windows Arm64・Windows 以外・wasm32 の IR とすべてのヘッダーが変更の前後で同じ。Windows x86-64 の IR の差は末尾の補助の部分だけ。
- [ ] テスト計画 1〜3 の新しい検査が通る。Windows で `integer_format.mjs` の照合が動く（S5 に当たらなければ）。T2 が通る。
- [ ] `platforms.md` の既知の制限を消し、`check-docs` が通る。PR に `ci:full` を付け、`windows.yml`（x64・Arm64）が通る。

## 落とし穴

- `-O3` で定数のプログラムだけを試すと、畳み込みで不具合が隠れる。オペランドは標準入力か C のホストから渡す。
- `linkonce_odr` にしない。`-O3` で、backend が最適化の後に作る libcall の相手（`__divti3`）が先に消える（W39 の測定）。
- 包みの `load` の `align 16` は、clang の C の ABI と LLVM の一時領域に合わせたものである。MinGW の GCC がホストを作った場合の整列は確かめていない（未確認）。
- この版より前のコンパイラで作った object は、誤った形の弱い `__divti3` を持つ。新しい object と同じホストに入れると、どちらが選ばれるかは確かめていない（未確認）。object は作り直す。
- `tests/windows.rs` の `cross_link` と Net の cross link（`TSUZURI_WINDOWS_SDK` のとき）は、scope なしで 1 つの IR を作り、`x86_64-pc-windows-msvc`（Net は `aarch64-pc-windows-msvc` も）でコンパイルする。補助の形はビルド機の arch になるが、`i128` を割らないので通ってしまう。`cross_link` は `x86_64-pc-windows-msvc` の `PlatformScope` に入れ、Net は arch ごとに scope を作って IR を出す。
- 数値ランタイムの object はキャッシュされるが、補助は利用者の module の側にあるので、キャッシュの鍵を変える必要はない。

## 対象外

- 引き戻し法の除算の速さ（compiler-rt と比べるなら、計測してから別のチケットにする）。
- `integer.ll` の除算を避ける実装（D6 のとおり残す）。
- COFF の弱い定義の既定の名前の衝突（[W39](W39-windows-multiple-objects-weak-defaults.md)）。起票時に、公開の強い記号を持たない試作の object の弱い定義（`.weak.__udivti3.default`）が、同梱の MinGW のリンクで zig の `compiler_rt.lib` の同じ名前と衝突した。W39 の範囲として記録だけする。
- 32-bit の Windows（target として受けない）。`--backend fast` も `udiv i128` を `__udivti3` の呼び出しにする（`src/backend/module.rs`）が、Windows では `E2000` で届かない。`--emit bitcode` も Windows では `E2000`（`BuildOptions::validate_bitcode`）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | Windows x86-64 では、補助を Win64 の C の ABI の形の包みで定義する | 試作で、両方のリンカー・同梱の MinGW・`-O0`/`-O3` で正しかった。コンパイラだけで閉じる（MSVC に compiler-rt はなく、`--emit object` の利用者は link.exe だけでリンクできる）。C の `__int128` と同じ形なので、ホストの呼び出しも正しくなる。包みは `ptr` と `<2 x i64>` だけを使うので、IR の `i128` の引数の扱いが LLVM の版で変わっても影響を受けない。退けた案: compiler-rt を連結する（`clang_rt.builtins-x86_64.lib` は LLVM の配布物にあるが MSVC にはない。object の利用者に連結を求めることになる。MinGW では弱い定義が zig の compiler_rt より先に参照を満たすので、補助を外す経路も要る）。除算を emitter で内部の関数の呼び出しに下げる（LLVM の定数の除数の展開と畳み込みを失い、数値ランタイムとホストの libcall は誤ったまま）。文書に書くだけ（今と同じ） | 既定案（実装者はこの案に従う） |
| D2 | LLVM が x86-64 で呼ぶ 4 つだけでなく、9 つすべてを包む | 弱い定義はリンク全体から見える。ホストの C や compiler-rt 風のコードは `__udivmodti4`・`__multi3`・シフトを名前で呼べ、今の形ではそれも誤った（試作で確認。C から呼んだのは `__udivmodti4`・`__multi3`・`__ashlti3`・`__ashrti3` で、同じ形の `__lshrti3` は未確認）。費用は包み 5 つ | 既定案（実装者はこの案に従う） |
| D3 | 本体は `wasm.ll` の 9 行を固定の接頭辞で読み替えて使い、写しを作らない | 算術の正は 1 つだけにする。wasm32 の `runtimeChecks` が同じ本体を検査し続ける。テスト計画 1 が名前の集合の一致と読み替えの数を固定する。退けた案: 本体を写した自己完結のファイル（2 か所を直し忘れうる） | 既定案（実装者はこの案に従う） |
| D4 | Windows Arm64 は `wasm.ll` のまま変えない | C の ABI と LLVM の呼び出しがどちらも `i128` を 2 つのレジスタで渡し、今の定義と一致する（アセンブリの読解）。IR が 1 byte も変わらない。実行の確認は `windows.yml` の Arm64 の job が担う | 既定案（実装者はこの案に従う） |
| D5 | arch は `windows_abi` の中で `current_platform()` から読み、`windows_abi` と `emit_with_export_style` の署名は変えない | driver と test runner の呼び出しはどれも `PlatformScope` の中にあり、`src/llvm.rs` のほかの target の分岐と同じ形になる。名前を付けた `--target` も正しく選ぶ。scope のないテストはビルド機の arch になるので、形を検査するテストは scope を作る | 既定案（実装者はこの案に従う） |
| D6 | `integer.ll` の「`i128` を割らない」規則とその 2 つの検査は残し、理由の注釈を「除算の補助を呼ばない設計」に書き直す | 正しさの理由はなくなるが、規則を外すと表示と `Parse` の生成コードが変わり、このチケットの範囲（補助の規約）を超える。今の実装は 10^9 の桁の筆算で、`i128` を表示するだけのプログラムは除算の補助を呼ばない。速さの比較は計測していない | 既定案（実装者はこの案に従う） |
