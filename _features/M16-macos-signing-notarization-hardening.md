# M16: Developer ID 署名・hardened runtime・公証（`tsuzuri notarize`）と生成コードの硬化の記録（W30 の macOS 版）

| 項目 | 内容 |
| --- | --- |
| ID | M16 |
| 優先度 | P2（Gatekeeper の下で配るアプリには Developer ID の署名・hardened runtime・公証が要る。今はすべて外部の手順で、何が有効かの記録もない） |
| 規模 | M |
| 依存 | [M15](_completed/M15-macos-app-bundle.md) |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-26（D-67。利用者の依頼「「どのチケットにもない壁」と「調査中に見つけたコンパイラの不具合（自分で再現済み）」のそれぞれについて @_features/ に開発チケットまたは修正チケットを作成して」から起票。調査で報告した「配布 1（署名と公証は外部の手順。生成コードに ssp がない。ad-hoc の dylib は hardened runtime のホストで拒まれる）」に当たる。HEAD `804b7970` で確認） |
| 承認 | 要承認: [GUIDE D-67](GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22) の D-55 の登録（`[macos]` の `sign_identity`・`entitlements`・`hardened_runtime`・`timestamp`、サブコマンド `tsuzuri notarize`、環境変数 `TSUZURI_CODESIGN`・`TSUZURI_NOTARYTOOL`）。生成コードへの `ssp` の D3（測定の後）。決定事項 D3・D4 が要承認、ほかは既定案 |
| 改善する劣位 | Xcode は「Signing & Capabilities」で Developer ID の署名・hardened runtime・entitlements を付け、Organizer か `xcrun notarytool` で公証する。Go・Rust は外部の道具（`gon`・`rcodesign` など）に頼る。Tsuzuri の出力はリンカーの ad-hoc の署名（Identifier は `artifact`）だけで、hardened runtime・entitlements・公証の口がない。生成コードには `ssp` がなく、ランタイムの C だけが Apple clang の既定の `ssp` を持つ。どれも記録も検査もされていない |
| 手本にする既存実装 | [W30](W30-windows-hardening.md)（硬化の状態の記録・固定のテスト・既定の変更を要承認にする書き方）、[M15](_completed/M15-macos-app-bundle.md) の `app_bundle::build_app` と `SignPlan`（内側からの署名）、`src/driver.rs` の `resolve_tool`・`tool`（環境変数 → 配布物 → PATH。`src/driver.rs:6918-6940`）・`run_tool`、`src/llvm_traps.rs` の `with_function_attribute`（関数の属性を IR に足す。`src/llvm_traps.rs:449`）、`tsuzuri audit`・`publish` のサブコマンドの解析（`src/main.rs:4033・4045`） |
| 主な影響ファイル | `src/app_bundle.rs`, `src/package.rs`, `src/driver.rs`, `src/main.rs`, `src/notarize.rs`（新規）, `src/lib.rs`, `src/diagnostic_kinds.rs`, `src/llvm_traps.rs`（D3 の承認の後だけ）, `tests/macos_signing.rs`（新規）, `tests/macos_hardening.mjs`（新規）, `tests/macos_bundle.mjs`, `.github/workflows/targets.yml`, `_tsuzuri/language-reference/compiler/platforms.md`, `_tsuzuri/language-reference/organizing-tsuzuri/packages.md`, `_tsuzuri/language-reference/compiler/usage.md`, `_tsuzuri/language-reference/compiler/option.md`, `_tsuzuri/language-reference/compiler/native-interop.md`, `_tsuzuri/language-reference/compiler/diagnostics.md`（生成）, `README.md`, `docs/architecture.md`, `docs/benchmarks.md`（D3 の測定） |

## 実装者向け早見

- 範囲:
  - `--emit app`（M15）の署名の段に、`[macos]` の `sign_identity`・`entitlements`・`hardened_runtime`・`timestamp` を効かせる。`codesign --force --sign <identity> [--options runtime] [--timestamp] [--entitlements <file>]` を、内側（`Contents/Frameworks`）から順に走らせる。
  - `tsuzuri notarize Name.app --keychain-profile <名前>`: 署名を事前に検査し、`ditto -c -k --keepParent` で zip にし、`notarytool submit --wait`、`stapler staple` と `stapler validate` を走らせる。秘密は受け取らない。
  - 硬化の状態（PIE、W+X のセグメントがない、実行できるスタックがない、JIT がない、hardened runtime の下でランタイムが動く）をテストで固定し、`platforms.md` に記録する。
  - 生成コードの `ssp` は測ってから決める（D3）。決まるまでは記録だけにする。
- 最初に読む: 「現状」の表 → 仕様 1〜3 → M15 の仕様 5 と `SignPlan`。
- 最初に書くテスト: `tests/macos_hardening.mjs` の段 H1（今の HEAD で通る基準）と、`tests/macos_signing.rs` の `macos_signing_keys_parse`。
- 触ってはいけない:
  - 署名のキー（`sign_identity`・`entitlements`・`hardened_runtime`・`timestamp`）を 1 つも書かない `--emit app` の署名（M15 の ad-hoc のまま）。
  - `--emit exe`・`shared`・`object` の出力とリンク行。リンカーの旗は足さない（D8）。
  - 生成 IR。D3 の承認までは `ssp` を付けない。
  - Linux・Windows・WASM の出力（D3 で `ssp` を採ったときの生成 IR を除く）。
- 完了の見分け方:
  - `[macos] sign_identity = "-"`・`hardened_runtime = true`・`entitlements = "app.entitlements"` の `--emit app` で、`codesign -dv` の flags が `adhoc,runtime` になり、`codesign -d --entitlements - --xml` が entitlements を出し、`--verify --deep --strict` が通り、`tests/fixtures/net_tzroutine` のアプリが動く。
  - `tsuzuri notarize` を ad-hoc の bundle に使うと、zip も送信もせずに `E2000` `notarize_signature_unsuitable` で止まる。
  - `tests/macos_hardening.mjs` が macOS の `-O0`・`-O3` で通る。

## 目的

Gatekeeper は、インターネットから取得したアプリに Developer ID の署名と公証を求める。公証の条件は、hardened runtime、安全なタイムスタンプ、Developer ID の署名である。M15 の bundle は ad-hoc の署名なので、作ったマシンでは動くが、配った先では開けない。

今は利用者が `codesign` と `notarytool` を自分で順に走らせる。内側から署名しないと署名そのものが失敗する（[M15](_completed/M15-macos-app-bundle.md) の再現 5）。hardened runtime と署名の組み合わせを誤ると、署名が通っても起動しない（「現状」の再現 3）。

硬化の状態も記録がない。`nm -u` に `___stack_chk_*` が出るので生成コードも stack protector を持つと読み違えやすいが、実際に持つのはランタイムの C だけである（再現 1）。W30 は Windows だけを扱い、POSIX は対象外である（W30:19・22・154）。

このチケットは、署名の段を M15 の bundle の組み立ての最後に置き、公証をサブコマンドにし、硬化の状態を測って固定する。

## 着手条件と停止条件

### 着手条件

- [M15](_completed/M15-macos-app-bundle.md) が done（`--emit app`、`[macos]` の節、`crate::plist`、`app_bundle::SignPlan` がある）。
- 承認が記録されるまで、該当する段に着手しない。手順 3〜5（キー・サブコマンド・環境変数）は D4、手順 7（生成コードの `ssp`）は D3 の承認が要る。手順 1・2・6（硬化の記録とテスト）は承認なしで進めてよい。
- `node scripts/repo-kb.mjs brief M16` を読む。
- 本物の Developer ID と公証は、CI でもこのチケットの作業でも使わない。手で確かめる手順を書き、確かめていなければ「未確認」と記録する。
- PR に `ci:full` を付ける。

### 停止条件

- **S1** hardened runtime の ad-hoc の署名で、hello・`examples/tzroutines`・`tests/fixtures/net_tzroutine`・`tests/fixtures/preemption`（`TSUZURI_MAXPROCS=1` の `tight`）のどれかが、署名しない場合と違う結果になる。
- **S2** 硬化の検査（仕様 4）が今の HEAD の出力で通らない（W+X のセグメント、`MH_ALLOW_STACK_EXECUTION`、PIE でない、など）。今の出力の性質を誤って記録していることになる。
- **S3** 署名の段のために、M15 の bundle の形かリンク行を変える必要が出た。
- **S4** D3 の測定で、`ssp` か `sspstrong` を付けた生成コードの IR が Clang に受け付けられないか、`-O0` と `-O3` の結果が違う。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `804b7970` で確認）

macOS 27.2.1（arm64）、Apple clang 21、Apple ld（ld-27106.1）、`target/debug/tsuzuri`、Xcode Command Line Tools で確かめた。本物の Developer ID と公証の申請は使っていない。

### コード

- 署名・公証・entitlements を扱うコードと文書はない（`rg -n 'codesign|notar|entitlement' src docs _tsuzuri` で 0 件）。`_features` では、ツールチェーンの配布物の公証を G14 の D9（Phase 2、要承認）が扱うだけである（`_features/_completed/G14-toolchain-distribution.md:12`）。
- ランタイムに `PROT_EXEC`・`MAP_JIT`・`pthread_jit_write_protect_np` はない（`rg -n 'PROT_EXEC|MAP_JIT|pthread_jit' src/runtime` で 0 件）。メモリーの写像は tzroutine のスタックだけで、`PROT_READ | PROT_WRITE` で写し（`src/runtime/tzroutine.c:823`）、低い端の `TZR_GUARD` を `PROT_NONE` にする（同 825 行）。`sigaltstack` は `src/runtime/stack.c:144` と `src/runtime/tzroutine.c:2646-2651`。`dlopen` は GPU のランタイムだけにある（`src/runtime/gpu-vulkan.c:932`、`src/runtime/gpu.c:137`）。macOS の既定の候補は Homebrew の絶対パスだけで（`src/runtime/gpu-vulkan.c:935-939`、`src/runtime/gpu.c:138`）、`TSUZURI_VULKAN_LIBRARY`・`TSUZURI_WEBGPU_LIBRARY` があればそのパスを開く（`src/runtime/gpu-vulkan.c:954`、`src/runtime/gpu.c:647`）。
- `-fno-stack-protector` は WebAssembly のランタイムの C だけに付く（`src/driver.rs:5085・5131・5173・5210`、`src/test_runner.rs:907`）。native のランタイムの C は Apple clang の既定でコンパイルされる。生成 IR に `ssp` を足すコードはない。関数の属性を IR に足す手本は `with_function_attribute`（`src/llvm_traps.rs:449`）で、`with_unwind_tables`（Windows の `uwtable`。`src/llvm_traps.rs:445`、呼び出しは `src/llvm.rs:368`）と `with_frame_pointers`（B10。`src/llvm_traps.rs:439`）が使う。
- `resolve_tool`（`src/driver.rs:6918-6936`）は、環境変数（空でも設定済み）、配布物の `bin/`、PATH の順に道具を探す。`TSUZURI_CODESIGN`・`TSUZURI_NOTARYTOOL` はない。
- W30 の範囲は Windows の native の実行ファイルで、POSIX と WASM の出力を触らず、POSIX の硬化を対象外にしている（W30:19・22・154）。

### 再現 1: 今の出力の性質

`hello`、`examples/tzroutines`、`tests/fixtures/net_tzroutine` を既定の native でビルドした。`hello` は次の `h/Main.tz` で、`tsuzuri build h/Main.tz -o hello` で作った。

```tsuzuri
def main :: unit -> i32 = \() ->
    do! IO.writeln "hello"
    0
```

| 項目 | hello | tzroutines | net_tzroutine |
| --- | --- | --- | --- |
| `codesign -dv` | `flags=0x20002(adhoc,linker-signed)`、`Identifier=artifact` | 同じ | 同じ |
| `otool -hv` の旗 | `NOUNDEFS DYLDLINK TWOLEVEL PIE` | `… PIE MH_HAS_TLV_DESCRIPTORS` | `… WEAK_DEFINES BINDS_TO_WEAK PIE MH_HAS_TLV_DESCRIPTORS` |
| セグメントの `maxprot`/`initprot` | `__TEXT` 5/5、`__DATA_CONST` 3/3、`__DATA` 3/3、`__LINKEDIT` 1/1 | 同じ | 同じ |
| `nm -u` の `___stack_chk_*` | なし | `___stack_chk_fail`・`___stack_chk_guard` | 同じ |
| `nm -u` の `mmap`・`mprotect`・`sigaltstack` | なし | あり | あり |

- 書き込みと実行を同時に許すセグメントはない。`MH_ALLOW_STACK_EXECUTION` はない。
- `--emit llvm` の `ssp`・`sspstrong`・`sspreq` は、hello（104 個の `define`）で 0 件、tzroutines（68 個の `define`）でも 0 件だった。tzroutines の `define` の属性は `nounwind`・`alwaysinline`・`noinline`・`"probe-stack"="inline-asm"`（TZ02 のスタックの探り）だけである。
- Apple clang 21 が C の関数に付ける既定は `ssp` と `"stack-protector-buffer-size"="8"`（`clang -O2 -S -emit-llvm` で確かめた）。tzroutines の `___stack_chk_*` はランタイムの C から来る。

### 再現 2: hardened runtime の ad-hoc の署名

```text
$ cp net net.hr && codesign --force --sign - --options runtime net.hr
$ codesign -dv net.hr 2>&1 | grep -o 'flags=[^ ]*'
flags=0x10002(adhoc,runtime)
$ codesign --verify --strict net.hr && echo verified
verified
```

| プログラム | 署名しない | hardened runtime（ad-hoc） |
| --- | --- | --- |
| hello | `hello`、終了コード 0 | 同じ |
| tzroutines | `21325334000`、0 | 同じ |
| net_tzroutine（ループバックで 100 本の接続） | `echoes: 100 read: TimedOut, others ran: true read across a close: closed`、0 | 同じ |
| preemption の `tight`（`TSUZURI_MAXPROCS=1`、`-O3`。呼び出しのないループを SIGURG で止める） | `tight: ticks=50`、0 | 同じ |

- entitlements（`com.apple.security.cs.disable-library-validation`）を付けた ad-hoc の署名も通り、`codesign -d --entitlements - --xml` がその plist を出した。
- `--timestamp` を ad-hoc に付けても、`codesign` は誤りを出さず、署名にタイムスタンプも入らなかった（`codesign -dv --verbose=4` に `Timestamp=` の行がない）。

### 再現 3: ライブラリの検証（library validation）

C の dylib（ad-hoc の linker-signed、install name は絶対パス）を `--link` でリンクした実行ファイル `lvapp` を使う。`ent.plist` は `com.apple.security.cs.disable-library-validation` を `<true/>` にした XML の plist（最上位の辞書に 1 つのキー）である。

```tsuzuri
@trusted
extern "m16_lib" def m16_lib :: unit -> i32

def main :: unit -> i32 = \() -> m16_lib ()
```

```text
$ printf 'int m16_lib(void){return 7;}\n' > dl/lib.c
$ clang -dynamiclib -install_name "$PWD/dl/libm16.dylib" dl/lib.c -o dl/libm16.dylib
$ tsuzuri build lv/Main.tz --link dl/libm16.dylib -o lvapp
$ cp lvapp lvapp.hr && cp lvapp lvapp.dlv
$ ./lvapp; echo $?
7
$ codesign --force --sign - --options runtime lvapp.hr && ./lvapp.hr
dyld[26636]: Library not loaded: …/dl/libm16.dylib
  Reason: tried: '…/dl/libm16.dylib' (code signature in <05C7246A-…> '…/dl/libm16.dylib' not valid for use in process: mapping process and mapped file (non-platform) have different Team IDs), …
$ echo $?
134
$ codesign --force --sign - --options runtime --entitlements ent.plist lvapp.dlv && ./lvapp.dlv; echo $?
7
```

dylib を `codesign --force --sign -` で署名し直しても（flags `adhoc`）、`--options runtime` を付けて署名し直しても、`lvapp.hr` は 134 のままだった。ad-hoc の署名には Team ID がないので、hardened runtime のプロセスは、どう署名した ad-hoc の dylib も読めない。同じ Team ID の Developer ID で両方を署名すれば通るはずだが、確かめていない。

GPU のランタイムも同じ規則に当たる。次のプログラムは、署名しなければ `vulkan sum=24`（Homebrew の `/opt/homebrew/lib/libvulkan.1.dylib` と MoltenVK）、hardened runtime の ad-hoc の署名では `vulkan unavailable`（終了コード 3）、`ent.plist` を付けると `vulkan sum=24` に戻った。

```tsuzuri
def scale :: i32 -> i32
fn scale value = value * 2i32 + 3i32

def main :: unit -> i32 = \() ->
    match Gpu.request Gpu.Vulkan with
    | Result.Ok device ->
        let buffer = Gpu.init (ref device) 4 (\index -> index as i32)
        let values = Gpu.to_array (Gpu.map (ref device) scale buffer)
        let total = Array.sum (ref values)
        do! IO.writeln $"vulkan sum={total}"
        0
    | Result.Error _ ->
        do! IO.writeln "vulkan unavailable"
        3
```

### 再現 4: 道具

| 道具 | 場所 |
| --- | --- |
| `codesign` | `/usr/bin/codesign`（OS） |
| `ditto` | `/usr/bin/ditto`（OS） |
| `notarytool` | `xcrun --find notarytool` は `/Library/Developer/CommandLineTools/usr/bin/notarytool`。`/usr/bin/notarytool` はない（`xcrun` からしか呼べない）。版は 1.1.4 |
| `stapler` | `/usr/bin/stapler`（`xcrun` の shim）と CLT |

`Probe.app` は M15 の再現 2 で手で組んだ bundle（ad-hoc で署名し直したもの）である。

```text
$ ditto -c -k --keepParent Probe.app Probe.zip         # 15 個の項目（._Contents・._Info.plist など AppleDouble の項目 7 個を含む）
$ xcrun notarytool submit Probe.zip --keychain-profile tsuzuri-m16-missing-profile --wait --output-format json
Error: No Keychain password item found for profile: tsuzuri-m16-missing-profile

Run 'notarytool store-credentials' to create another credential profile.
$ echo $?
69
$ xcrun stapler validate Probe.app
Processing: …/Probe.app
Probe.app does not have a ticket stapled to it.
$ echo $?
65
```

`--output-format json` を付けても、資格情報の誤りは標準エラーの文で出て、標準出力は空だった。`notarytool submit --help` は `-p, --keychain-profile`・`--wait`・`--timeout`・`-f, --output-format` を挙げる。`notarytool log --help` によれば、`log <submission-id> [<output-path>]` は出力のパスを省くと標準出力に JSON を書き、`--output-format` はその形に効かない。公証が通ったときの JSON の形（`id`・`status`）と、`Invalid` のときの終了コードは確かめていない。

`codesign -d`（`-dv --verbose=4` を含む）は表示を標準エラーに書き、標準出力は空だった。`run_tool` は標準出力と標準エラーをつないで返す（`src/driver.rs:6963` の `tool_output`）。

## 変えてはいけない意味

- 署名のキーを 1 つも書かない `--emit app` は、M15 の ad-hoc の署名のまま（同じ `codesign` の引数、同じバイト列）。`sign_identity` を書かずに `hardened_runtime` か `entitlements` だけを書いたときは、ad-hoc のままその引数を足す（完了の見分け方と G1）。
- 実行ファイル・共有ライブラリ・object のリンク行と生成 IR。D3 の承認まで、IR は 1 byte も変えない。
- Linux・Windows・WASM の出力。例外は D3 で `ssp` を採ったときだけで、native の全 target の生成 IR に関数の属性が付く（D-67 の 1 byte の規則の例外として、D3 の承認のときに諮る）。

## 仕様

### 1. `[macos]` の署名のキー

```toml
[macos]
bundle_id = "dev.example.probe"
sign_identity = "Developer ID Application: Example Corp (ABCDE12345)"
entitlements = "app.entitlements"
hardened_runtime = true
timestamp = true
```

| キー | 値 | 誤り（`E0002` `invalid_macos_setting`） |
| --- | --- | --- |
| `sign_identity` | 空でない文字列。`"-"` は ad-hoc。それ以外はそのまま `codesign -s` に渡す（名前か SHA-1 の 16 進） | `sign_identity must be nonempty and must not contain control characters` |
| `entitlements` | 相対パス（XML の plist で、最上位は辞書） | `macos paths must be nonempty relative paths` |
| `hardened_runtime` | `true`・`false` | `hardened_runtime must be true or false` |
| `timestamp` | `true`・`false`。`true` は `sign_identity` が `"-"` でないときだけ | `timestamp needs a sign_identity other than "-"; ad-hoc signatures carry no timestamp` |

- 省略したときの値: `sign_identity` は ad-hoc（M15）、`hardened_runtime` と `timestamp` は `false`、entitlements はなし。公証に要る 3 つを黙って補わない（D5）。
- 知らないキーの文は M15 の一覧の後ろに `sign_identity, entitlements, hardened_runtime, or timestamp` を足す。
- `entitlements` の中身は M15 の `crate::plist::read` で読む。読めなければ `E2001` `macos_bundle_input_invalid`。
- `--emit app` 以外の出力では読まない（M15 の `[macos]` と同じ）。

### 2. 署名の段（`--emit app` の最後）

- 順は M15 と同じく内側から。`embed` の各 Mach-O のファイルと各 `.framework` に、次に bundle に `codesign` を走らせる。
- 引数: `--force --sign <identity>`。`hardened_runtime` なら `--options runtime`、`timestamp` なら `--timestamp`。`--entitlements <file>` は bundle（本体の実行ファイル）にだけ付け、内側のコードには付けない（D7）。`--deep` は使わない。
- identity の文字列はシェルを経ずに `Command` の引数として渡す（D1）。
- `codesign` は `resolve_tool("TSUZURI_CODESIGN", "codesign")` で探す（空でも設定済み）。M15 の ad-hoc の段も同じ関数で探すように改める。
- `sign_identity` が `"-"`（か省略）、`hardened_runtime` が `true`、`embed` が空でなく（M15 の `embed` は Mach-O のファイルと `.framework` だけで、どちらもコード）、entitlements に `com.apple.security.cs.disable-library-validation` の `true` がなければ、`E2000`（kind 案 `macos_signing_conflict`、文案 `an ad-hoc signature with hardened_runtime cannot load the embedded 'libfoo.dylib': dyld requires the same Team ID; sign with a Developer ID or add com.apple.security.cs.disable-library-validation to the entitlements`）。`embed` したコードを読み込んだ時点で dyld が必ず拒む組み合わせを、ビルドの時点で止める（再現 3。D6）。
- `codesign` の失敗は `E2002` `external_tool_failed` で、道具の出力を付ける（identity が keychain にない、タイムスタンプのサーバーに届かない、など）。`--timestamp` にはネットワークが要る（文書に書く）。

### 3. `tsuzuri notarize`

```text
tsuzuri notarize Name.app --keychain-profile <名前>
```

1. ホストが macOS でなければ `E2000` `option_unsupported_platform`（文案 `notarize needs a macOS host with the Xcode Command Line Tools`）。
2. 引数はちょうど 1 つの、`.app` で終わるディレクトリ。`--keychain-profile` は必須で 1 回だけ。違えば `E2000`（既存の `usage_error`・`option_missing_value`・`duplicate_option`）。`--keychain-profile` がない文は `notarize needs --keychain-profile NAME; create the profile once with xcrun notarytool store-credentials`。
3. 事前の検査。`codesign --verify --deep --strict <app>` が通り、`codesign -dv --verbose=4 <app>` に次の 3 つがあること:
   - `Authority=Developer ID Application:` で始まる行。
   - `flags=` の括弧の中の `runtime`。
   - `Timestamp=` の行（安全なタイムスタンプ）。

   どれかが欠ければ、送信せずに `E2000`（kind 案 `notarize_signature_unsuitable`、文案 `'Name.app' is not ready for notarization: it needs a Developer ID Application signature, the hardened runtime and a secure timestamp; set [macos] sign_identity, hardened_runtime = true and timestamp = true`。欠けたものだけを挙げる）。
4. `ditto -c -k --keepParent <app> <一時ディレクトリ>/<Name>.zip`。一時ディレクトリは `<app>` の横の `TemporaryDirectory`。
5. `<notarytool> submit <zip> --keychain-profile <名前> --wait --output-format json`。`<notarytool>` は `TSUZURI_NOTARYTOOL` があればその実行ファイル、なければ `xcrun notarytool`。
   - 標準出力と標準エラーを分けて受ける（`run_tool` はつないで返すので使わず、`Command::output` を直接読む）。
   - 標準出力が `id` と `status` を持つ JSON なら、終了コードによらず `status` で決める（`Invalid` のときの終了コードは確かめていない。再現 4）。`Accepted` なら次へ。それ以外（`Invalid`・`Rejected` など）なら、`<notarytool> log <id> --keychain-profile <名前>` の JSON の `issues` の先頭 20 件を付けて `E2002`（kind 案 `notarization_rejected`、文案 `notarization of 'Name.app' ended with status Invalid (submission <id>)`）。
   - 標準出力をそう読めなければ `E2002` `external_tool_failed` にし、終了コードと標準エラーの文（再現 4 の資格情報の誤りなど）をそのまま付ける。終了コードが 0 でも同じ。
6. `xcrun stapler staple <app>`、次に `xcrun stapler validate <app>`。失敗は `E2002` `external_tool_failed`。
7. 成功したら標準出力に `notarized 'Name.app' (submission <id>); the ticket is stapled` を書き、終了コード 0。

- 秘密（Apple ID、アプリ用パスワード、API キー）は引数・manifest・環境変数のどれでも受け取らない。`notarytool store-credentials` で keychain に入れた profile の名前だけを渡す（D2）。
- `notarize` は bundle を作り直さない。`--emit app` の後に走らせる。

### 4. 硬化の状態の固定（macOS の native の実行ファイル）

テスト `tests/macos_hardening.mjs` が、hello・`examples/tzroutines`・`tests/fixtures/net_tzroutine`・`tests/fixtures/preemption` を `-O0`・`-O3` でビルドし、次を確かめる（D8）。

| 性質 | 確かめ方 | 期待（今の値） |
| --- | --- | --- |
| PIE | `otool -hv` の旗 | `PIE` がある |
| 実行できるスタック | 同上 | `MH_ALLOW_STACK_EXECUTION` がない |
| W+X | `otool -l` の各 `LC_SEGMENT_64` | `maxprot` に 2 と 4 を両方持つセグメントがない |
| JIT | `nm -u` | `_pthread_jit_write_protect_np` がない |
| hardened runtime | ad-hoc の `--options runtime` で署名して実行 | 再現 2 の表と同じ出力と終了コード |
| ライブラリの検証 | 再現 3 の `lvapp` | hardened runtime で 134、entitlements 付きで 7 |
| ランタイムの C の `ssp` | tzroutines の `nm -u` | `___stack_chk_guard` がある |
| 生成コードの `ssp` | `--emit llvm` | D3 の決定まで、`ssp`・`sspstrong`・`sspreq` が 0 件 |

- Rust のテスト `runtime_has_no_executable_mappings` が、`src/runtime` の C に `PROT_EXEC`・`MAP_JIT`・`pthread_jit_write_protect_np` がないことを確かめる（将来の変更を捕まえる）。
- 結果は `platforms.md` の新しい節「macOS の実行ファイルのセキュリティ機能」に表で記録する。リンカーの旗は足さない（どれも既定で有効）。

### 5. 生成コードの `ssp`（D3。要承認）

- 測定: `with_function_attribute` で生成 IR の全関数に `ssp`（Apple clang の C の既定と同じ）を足す版と、`sspstrong` を足す版を作り、`examples` と `benches` の `-O0`・`-O3` の実行時間と実行ファイルの大きさを、付けない版と比べる（`docs/benchmarks.md` の手順。共有の CI に合否の閾値は入れない）。
- 決定の材料: 生成コードは境界検査で守られ、スタックの配列への範囲外の書き込みは言語の意味で起きない。利益は同じプロセスの C から来る破壊の検出に限られる。
- 承認の後に付けるなら、関数の属性として native の全 target（`-O0`・`-O3`）に付け、WASM には付けない。それまでは仕様 4 の表に「なし」と記録するだけにする。

### 6. 診断

| 条件 | コード | kind | 文の例 |
| --- | --- | --- | --- |
| 署名のキーの形 | `E0002` | `invalid_macos_setting`（M17・M15 と共有） | 仕様 1 の表 |
| ad-hoc と hardened runtime と `embed` のコード | `E2000` | `macos_signing_conflict`（新しい kind の案） | 仕様 2 |
| `notarize` のホスト | `E2000` | `option_unsupported_platform`（既存） | 仕様 3 |
| `notarize` の引数 | `E2000` | `usage_error`・`option_missing_value`・`duplicate_option`（既存） | 仕様 3 |
| 公証に向かない署名 | `E2000` | `notarize_signature_unsuitable`（新しい kind の案） | 仕様 3 |
| entitlements の中身 | `E2001` | `macos_bundle_input_invalid`（M15） | M15 の仕様 4 の読み手の文 |
| `codesign`・`ditto`・`notarytool`・`stapler` の失敗 | `E2002` | `external_tool_failed`・`external_tool_not_runnable`（既存） | 道具の出力を付ける |
| 公証が受理されない | `E2002` | `notarization_rejected`（新しい kind の案） | 仕様 3 |

## 設計

| ファイル | 変更 |
| --- | --- |
| `src/package.rs` | `MacosSettings` に `sign_identity: Option<String>`・`entitlements: Option<PathBuf>`・`hardened_runtime: Option<bool>`・`timestamp: Option<bool>`。`timestamp` と ad-hoc の検査は節を読み終えたときに行う |
| `src/app_bundle.rs` | `SignPlan { identity: String, entitlements: Option<PathBuf>, hardened_runtime: bool, timestamp: bool }`（M15 は `identity: "-"` だけを使う）。`fn codesign_args(plan, path, is_bundle) -> Vec<OsString>`（純粋な関数で、テストが引数の列を確かめる）。仕様 2 の衝突の検査 |
| `src/notarize.rs`（新規） | `pub fn notarize(app: &Path, profile: &str) -> Result<String, Diagnostic>`。`fn readiness(codesign_display: &str) -> Vec<Missing>`（`codesign -dv --verbose=4` の文を読む純粋な関数。表示は標準エラーに出る）、`fn submission(json: &str) -> Result<(String, String), String>`（`id` と `status`）、`fn issues(json: &str) -> Vec<String>`。JSON は既存の依存の `serde_json` で読む（新しい crate は足さない）。事前の検査の後の段（zip・送信・ステープル）は `#[doc(hidden)] pub fn submit_and_staple(app, profile, tools: &NotarizeTools)` に分け、`NotarizeTools` に `notarytool` と `stapler` の実行ファイルと前置きの引数を持たせる。CLI は `TSUZURI_NOTARYTOOL`（なければ `xcrun notarytool`）と `xcrun stapler` を渡し、テストは偽の道具を渡す |
| `src/main.rs` | サブコマンド `notarize` の解析と `HELP`（使い方の行、`TSUZURI_CODESIGN`・`TSUZURI_NOTARYTOOL` の段） |
| `src/driver.rs` | `resolve_tool` の呼び出し（新しい関数は要らない） |
| `src/diagnostic_kinds.rs` | 新しい kind 3 つ（`macos_signing_conflict`・`notarize_signature_unsuitable`・`notarization_rejected`） |

- `notarytool` の既定は `xcrun` に `notarytool` を前置きの引数として渡す（再現 4。`/usr/bin/notarytool` はない）。`stapler` は常に `xcrun stapler`（環境変数を足さない。D9）。
- 署名はキャッシュしない（M15 の D5 のとおり、bundle の組み立ては毎回行う）。`TSUZURI_CODESIGN` を `KEY_ENVIRONMENT`（`src/cache.rs:22-45`）に足さない。
- `notarize` の一時ディレクトリは、成功しても失敗しても消す。zip は bundle の横に残さない。

## 前提とする他チケットのインターフェース

- [M15](_completed/M15-macos-app-bundle.md): `--emit app`、`MacosSettings`、`crate::plist::read`、`app_bundle::build_app` の署名の段と `SignPlan`、内側からの署名の順、kind `macos_bundle_input_invalid`。
- [W30](W30-windows-hardening.md): 硬化の状態を表にして固定し、既定の変更を要承認にする書き方（Windows の旗は扱わない）。

## 他チケットへ提供するインターフェース

- `SignPlan` の 4 つの欄と `codesign_args`。[M18](M18-intel-mac-and-universal2.md) の Universal 2 の bundle も同じ段で署名する（M18 は D-67 の分岐 3 で blocked。方針を改める決定があったときの話である）。
- `platforms.md` の「macOS の実行ファイルのセキュリティ機能」の表。[M12](M12-gpu-presentation-to-window.md) が、bundle の中の GPU ランタイムを同じ Team ID で署名する手順の根拠に使う（再現 3 の GPU の行）。
- `native-interop.md` の「hardened runtime のホストに dylib を入れる」の段。`--emit shared` を Xcode のアプリに埋め込む利用者が読む。

## 実装手順

1. **変更**: `tests/macos_hardening.mjs`、`.github/workflows/targets.yml`（macOS の段）。**内容**: 仕様 4 の表の検査を今の値で書く。**確認**: macOS で `node tests/macos_hardening.mjs target/debug/tsuzuri` が HEAD で通る（基準。S2）。
2. **変更**: `tests/macos_signing.rs`（`runtime_has_no_executable_mappings` だけ）。**内容**: ランタイムの C の検査。**確認**: `cargo test --locked --test macos_signing`（`running N tests`）。
3. **変更**: `src/package.rs`、`tests/macos_signing.rs`。**内容**: 仕様 1 のキー。**確認**: `macos_signing_keys_parse`、T1。
4. **変更**: `src/app_bundle.rs`、`src/diagnostic_kinds.rs`、`tests/macos_bundle.mjs`。**内容**: 仕様 2（`codesign_args`、衝突の検査、`TSUZURI_CODESIGN`）。**確認**: `codesign_args_follow_the_plan`、`tests/macos_bundle.mjs` の G1〜G4 の段（テスト計画）、T1。
5. **変更**: `src/notarize.rs`、`src/main.rs`、`src/lib.rs`。**内容**: 仕様 3。**確認**: `notarize_readiness_reads_codesign_output`・`notarize_parses_submission`、CLI の N1 と、偽の `notarytool`・`stapler` を `submit_and_staple` に渡す N2〜N4（テスト計画の注記）、T1。
6. **変更**: なし（手での確認）。**内容**: Developer ID を持つ人が「テスト計画」の手での確認を行い、結果をこのチケットと `platforms.md` に記録する。持っていなければ「未確認」と記録する。**確認**: 記録。
7. **変更**（D3 の承認の後）: `src/llvm.rs` か `src/llvm_traps.rs`、`docs/benchmarks.md`。**内容**: 仕様 5 の測定を記録し、承認された形で付ける。**確認**: 仕様 4 の表の最後の行を改め、`-O0`・`-O3` の E2E（`tests/e2e.mjs`）が通る（S4）。
8. **変更**: 文書。**確認**: `gen-docs --check`・`check-cli-docs`・変更したページの `check-docs`・T2・`ci:full` を付けた PR の `targets.yml`。

## テスト計画

### Rust（`tests/macos_signing.rs`。どのホストでも走る）

- `macos_signing_keys_parse`: 仕様 1 の受理と各誤り。`timestamp = true` と `sign_identity = "-"`（と省略）の組は誤り。
- `codesign_args_follow_the_plan`: ad-hoc（M15 と同じ列）、Developer ID・runtime・timestamp・entitlements の列。entitlements は bundle にだけ付き、内側のコードには付かない。identity の文字列に空白と括弧があっても 1 つの引数のまま。
- `signing_conflict_is_reported`: ad-hoc と `hardened_runtime` と Mach-O の `embed` は `macos_signing_conflict`。entitlements に `disable-library-validation` があれば通る。
- `notarize_readiness_reads_codesign_output`: `codesign -dv --verbose=4` の文の fixture（ad-hoc、`adhoc,runtime`、Developer ID でタイムスタンプなし、3 つがそろったもの。最後のものは Apple の文書の形から作り、「本物の出力で未確認」と注記する）から、欠けたものを正しく挙げる。
- `notarize_parses_submission`: `{"id":"…","status":"Accepted","message":"…"}`、`Invalid`、壊れた JSON。`issues` の取り出しと 20 件の上限。
- `runtime_has_no_executable_mappings`。

### E2E（macOS だけ）

| 段 | 期待 |
| --- | --- |
| H1 `tests/macos_hardening.mjs`（`-O0`・`-O3`） | 仕様 4 の表のとおり |
| G1 `sign_identity = "-"`・`hardened_runtime = true`・entitlements（`com.apple.security.network.client`） | flags に `runtime`、`codesign -d --entitlements - --xml` に entitlements、`--verify --deep --strict` が通る、net_tzroutine のアプリが同じ出力 |
| G2 G1 に署名のない dylib の `embed` | `macos_signing_conflict`。entitlements に `disable-library-validation` を足すとビルドが通り、アプリが起動して終了コード 0 になる。M20 の後は `--rpath @executable_path/../Frameworks` でリンクした dylib の関数の値を出すことも確かめる |
| G3 `TSUZURI_CODESIGN` に引数を記録する偽の `codesign` | 内側から順に呼ばれ、仕様 2 の引数の列になる |
| G4 `timestamp = true` と `"-"` | `E0002` `invalid_macos_setting` |
| N1 `notarize` を ad-hoc の bundle に | `notarize_signature_unsuitable` で、zip を作らず `notarytool` を呼ばない（偽の `notarytool` が呼ばれた印を残さない） |
| N2 偽の `notarytool` が標準エラーに再現 4 の文、標準出力は空、終了コード 69 | `E2002` `external_tool_failed` で、その文と終了コードを含む |
| N3 偽の `notarytool` が `status` `Invalid` の JSON（終了コード 0 と 1 の両方）と、`log` で `issues` | `E2002` `notarization_rejected`、`issues` の文を含む。偽の `stapler` は呼ばれない |
| N4 偽の `notarytool` が `Accepted`、偽の `stapler` が成功 | 偽の `stapler` が `staple <app>`、`validate <app>` の順に呼ばれ、仕様 3 の 7 の文を出して成功する。偽の `stapler` が失敗すれば `E2002` `external_tool_failed` |

N1 は CLI で走らせる。N2〜N4 は ad-hoc の bundle では事前の検査を通らないので、設計の `submit_and_staple` を macOS の Rust のテストが偽の `notarytool` と偽の `stapler`（引数を記録するシェルスクリプト）を渡して呼ぶ。本物の `xcrun stapler` はネットワークで Apple に ticket を問い合わせるので、テストでは呼ばない。CLI に検査を飛ばす旗も、`stapler` を差し替える環境変数も足さない（D9）。本物の Developer ID の署名を CI で作らない。

### 手での確認（Developer ID を持つ人。記録だけ）

1. `xcrun notarytool store-credentials <profile> --apple-id … --team-id …`。
2. `[macos]` に Developer ID の `sign_identity`、`hardened_runtime = true`、`timestamp = true` を書き、`tsuzuri build . --emit app -o Probe.app`。
3. `tsuzuri notarize Probe.app --keychain-profile <profile>`、`spctl -a -vv -t exec Probe.app` が `source=Notarized Developer ID` を出す。
4. 同じ Team ID で署名した dylib を `embed` し、hardened runtime の下で読めること（再現 3 の逆）。

結果（通ったか、未確認か）をこのチケットの末尾と `platforms.md` に書く。

## ドキュメント

- `compiler/platforms.md`: 新しい節「macOS の実行ファイルのセキュリティ機能」（仕様 4 の表、確かめた機械と版、ライブラリの検証と GPU のランタイムの注意）と、配布の説明（署名・公証の手順、未確認の範囲）。
- `organizing-tsuzuri/packages.md`: `[macos]` の 4 つのキー（M15 の節に足す）。
- `compiler/usage.md`: サブコマンド `notarize`（使い方、`store-credentials` の手順、秘密を渡さないこと）。
- `compiler/option.md`: 「環境変数」の表に `TSUZURI_CODESIGN`・`TSUZURI_NOTARYTOOL`。
- `compiler/native-interop.md`: `--emit shared` の dylib を hardened runtime のホストに入れるときは、ホストと同じ Team ID で署名する（ad-hoc は再現 3 のとおり拒まれる。Developer ID での確認は手での確認の 4）。GPU のランタイムを hardened runtime の下で使う注意。
- `compiler/diagnostics.md`（`gen-docs`）、`README.md`（CLI の概要にサブコマンド `notarize`、環境変数）、`docs/architecture.md`（bundle の署名の段）、`docs/benchmarks.md`（D3 の測定）。

## 受け入れ条件

- [ ] 仕様 1〜3・6 のとおりに動く（Rust と E2E）。秘密を受け取る口がない。
- [ ] 仕様 4 の硬化の検査が macOS の `-O0`・`-O3` で通り、`platforms.md` に記録した。
- [ ] 署名のキーを書かない `--emit app`、ほかの出力、生成 IR が変わらない（生成 IR は D3 の承認まで）。
- [ ] Developer ID と公証を手で確かめたか、「未確認」と記録した。
- [ ] D3 の測定を記録した（承認されたら仕様 5 のとおりに付けた）。
- [ ] `gen-docs --check`・`check-cli-docs`・変更したページの `check-docs`・T2 が通り、`ci:full` を付けた PR の `targets.yml` が通る。

## 落とし穴

- **entitlements は本体だけ**: entitlements はプロセス（本体の実行ファイル）の性質で、dylib と framework に付けても効かない。`--entitlements` は bundle の最後の 1 回だけに付ける。
- **ad-hoc と hardened runtime**: 手元で hardened runtime を試すと、ad-hoc の dylib と Homebrew の dylib（GPU のランタイム）が読めない（再現 3）。不具合ではなくライブラリの検証の規則で、文書で説明する。`embed` でなく `--link`・`[native]` で bundle の外の dylib をリンクしたアプリ（再現 3 の `lvapp` の形）も同じ理由で起動しない。D6 の検査は `embed` だけを見るので、これは文書に書く。
- **`--timestamp` とネットワーク**: Apple のタイムスタンプのサーバーに届かないと署名が失敗する。オフラインの開発では `timestamp = false` にする。ad-hoc では効果がないので誤りにする（仕様 1）。
- **`notarytool` は `xcrun` から**: `/usr/bin/notarytool` はない。PATH の `notarytool` を探すと見つからない。
- **`--output-format json` でも文が出る**: 資格情報の誤りは JSON でなく標準エラーの文で出た（再現 4）。標準出力を JSON として読めたときだけ `status` を使い、読めなければ終了コードと標準エラーの文を付けて報告する（仕様 3 の 5）。終了コードだけで判断しない（`Invalid` のときの終了コードは確かめていない）。
- **zip の AppleDouble**: `ditto -c -k --keepParent` の zip には、拡張属性（OS が付ける `com.apple.provenance` を含む）を保つ `._Contents` などの項目が入る。Apple の手順どおりの形で、公証の妨げにならないとされる（未確認）。
- **`spctl` は Developer ID だけ**: ad-hoc の bundle を `spctl -a` すると常に拒まれる。CI の検査に使わない。
- **`ssp` の読み違い**: `nm -u` の `___stack_chk_*` は、ランタイムの C が連結されたかどうかで決まる。生成コードの保護の証拠に使わない（再現 1）。

## 対象外

- 秘密（Apple ID の資格）の扱い。keychain の profile の名前だけを持つ。
- App Sandbox と Mac App Store の提出、`.pkg`・`.dmg` の作成と公証。App Sandbox の entitlements（security-scoped bookmark を含む）と Mac App Store の提出（Apple Distribution の証明書、provisioning profile、App Store Connect への送信）は、Developer ID の配布とは経路が違い、起票の調査で確かめていない。要るときに別のチケットにする（起票していない）。
- arm64e（ポインター認証の ABI）。
- コンパイラ自身の配布物の公証（G14 の D9）。
- `--emit exe`・`--emit shared` の単体の署名（`codesign` を手で走らせる。文書に書く）。
- Linux の ELF の硬化（RELRO・CET の印）と Windows（W30）。
- `notarize` の `--json` の出力（要るなら後で D-55 に登録する）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 署名は M15 の bundle の組み立ての最後の段にし、`sign_identity` の文字列をシェルを経ずに `codesign -s` へ渡す。`"-"` は ad-hoc | 組み立てと署名を分けると、署名の後の変更で署名が壊れる（M15 の再現 2）。シェルを経ないので、identity の空白・括弧・引用符で誤らない。採らない案: 署名を別のサブコマンドにする（組み立て直しのたびに 2 つのコマンドが要る） | 既定案（実装者はこの案に従う） |
| D2 | `notarize` は秘密を受け取らず、`--keychain-profile` の名前だけを `notarytool` に渡す。`store-credentials` の手順は文書に書く | 秘密を manifest・引数・環境変数に置くと、履歴・ログ・プロセスの一覧に残る。keychain の profile は Apple の推奨の形。採らない案: `--apple-id`・`--password`・`--key` を中継する | 既定案（実装者はこの案に従う） |
| D3 | 生成コードの `ssp`・`sspstrong` は、`examples` と `benches` の `-O0`・`-O3` で測ってから決める。付けるなら関数の属性で native の全 target に付け、WASM には付けない。決めるまでは記録だけ | 生成コードは境界検査で守られ、利益は同じプロセスの C から来る破壊の検出に限られる。費用は関数ごとのカナリアの読み書き。AGENTS の「性能の主張は計測の後」。採らない案: 測らずに `sspstrong` を既定にする（費用の裏付けがない）、測らずに付けないと決める（利益と費用を比べていない） | 要承認（承認前は手順 7 に着手しない） |
| D4 | `[macos]` の `sign_identity`・`entitlements`・`hardened_runtime`・`timestamp`、サブコマンド `notarize`、環境変数 `TSUZURI_CODESIGN`・`TSUZURI_NOTARYTOOL` を足す（D-55） | 署名はアプリの性質なので manifest に置く（M15 と同じ節）。公証は bundle を作った後の別の操作で、ネットワークが要り時間もかかるのでサブコマンドにする。道具の変数は `TSUZURI_DSYMUTIL` などと同じ探し方。採らない案: 署名の値を `build` の旗にする（ビルドのたびに書くことになり、手元と CI で食い違う）、公証を `--emit app` の段に入れる（ビルドがネットワークと数分の待ちに縛られる） | 要承認（D-55。承認前は手順 3〜5 に着手しない） |
| D5 | `hardened_runtime`・`timestamp` の既定は `false` で、Developer ID から補わない。`timestamp` と ad-hoc の組は誤り。`notarize` は送信の前に署名を検査する | GUIDE §1 規則 2（暗黙の既定値を置かない）。明示した要求を黙って捨てない（規則 4）。公証の条件の欠けは、Apple に送って待つ前に分かる。採らない案: Developer ID の identity なら hardened runtime と timestamp を自動で有効にする（暗黙の既定値）、`timestamp` と ad-hoc の組を黙って無視する（明示した要求を捨てる） | 既定案（実装者はこの案に従う） |
| D6 | ad-hoc と hardened runtime と `embed` のコードの組み合わせは、`disable-library-validation` がなければ `E2000` | `embed` したコードを読み込むと、dyld が必ず拒む（再現 3。dylib をどう ad-hoc で署名し直しても 134）。`embed` は読み込むためのコードなので、ビルドの時点で理由と直し方を示す。GPU のランタイムの `dlopen` と `--link` の外の dylib はビルドの時点で区別しにくいので、文書だけにする。採らない案: 警告だけにする（必ず起動しない bundle を作る）、`disable-library-validation` を黙って足す（明示しない権限の緩和） | 既定案（実装者はこの案に従う） |
| D7 | 内側のコードは同じ identity・runtime・timestamp で署名し、entitlements は付けない | Team ID をそろえるとライブラリの検証を通る。entitlements はプロセスの性質で、本体の実行ファイルが持つ。採らない案: 内側のコードにも同じ entitlements を付ける（dylib と framework には効かない） | 既定案（実装者はこの案に従う） |
| D8 | 硬化はリンカーの旗を足さず、今の性質をテストで固定して記録する | PIE・W+X なし・実行できるスタックなしは、今の Apple ld の既定で成り立つ（再現 1）。旗を渡すとリンク行が変わるだけで、既定が外れたらテストが落ちる（W30 の D5 と同じ）。採らない案: `-Wl,-pie` などを明示で渡す（リンク行が変わるだけで、出力の性質は同じ） | 既定案（実装者はこの案に従う） |
| D9 | 公証は `ditto -c -k --keepParent`、`notarytool submit --wait --output-format json`、`stapler staple`・`validate` の順。`stapler` には環境変数を足さない | Apple の文書の手順どおり。`stapler` は `notarytool` と同じ CLT にあり、利用者が差し替える理由がない。テストは `submit_and_staple` に偽の `notarytool`・`stapler` を渡して両方の道を確かめ、本物の成功の道は手で確かめる。採らない案: `stapler` にも環境変数を足す（D-55 の登録が増えるだけ）、送信の後に待たない（ステープルできず、利用者が 2 回目のコマンドを走らせる必要がある） | 既定案（実装者はこの案に従う） |
