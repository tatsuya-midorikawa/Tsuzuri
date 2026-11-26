# W17: `extern` ごとのシステムライブラリ `@link` と `dllimport`

| 項目 | 内容 |
| --- | --- |
| ID | W17 |
| 優先度 | P2（Win32 の宣言のパッケージは `user32`・`gdi32`・`comctl32` を要るが、今は利用者が `-l` か `[native]` を自分で書くしかなく、git・registry のパッケージは `[native]` を持てない） |
| 規模 | M（属性の構文と符号化、検査、`declare` の `dllimport`、5 か所のリンク行、`tsuzuri audit` の出力、パッケージの規則、テストと文書） |
| 依存 | – |
| 後続 | [W27](W27-win32-package-from-winmd.md)（win32metadata の `DllImport` の写し先と、公式の `win32` パッケージの配布）, [M07](M07-apple-sdk-bindgen-and-package.md)（公式の `apple` パッケージのリンクの要求）（弱い後続: [M14](M14-tls-http-packages.md), [M20](M20-macos-framework-link-inputs.md)。M20 の Phase 2 が `@link(framework = "...")` に広げる） |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 要承認（E12 D6 の拡張、D-53）。決定事項 D1・D2 が要承認、D3〜D9 は既定案 |
| 手本にする既存実装 | `src/parser.rs` の `attributes`・`trusted_attribute`、`src/syntax.rs` の `Program::trusted`、`src/driver.rs` の `library_name_error`・`LinkInputs::add_to`・`build_complete` の `-lws2_32`（ランタイムが要るシステムライブラリを足す所）、`src/runtime_cache.rs` の `RuntimeSymbols`（IR の名前から要るものを決める形）、`src/llvm_imports.rs` の `host_call` の `declare`、`src/capabilities.rs` の `ExternReport` と `Report` の文字と JSON の出力 |
| 主な影響ファイル | `src/syntax.rs`, `src/parser.rs`, `src/syntax_codec.rs`, `src/frontend_cache.rs`（`FRONTEND_FORMAT`）, `src/formatter.rs`, `src/api_diff.rs`, `src/check.rs`, `src/llvm_imports.rs`, `src/llvm.rs`, `src/driver.rs`, `src/test_runner.rs`, `src/capabilities.rs`, `src/diagnostic_kinds.rs`, `tests/link_attribute.rs`（新規）, `tests/ffi_extensions.mjs`, `tests/packages.rs`, `tests/windows.mjs`, `docs/language.md`, `_tsuzuri/language-reference/compiler/native-interop.md`, `_tsuzuri/language-reference/values-and-functions/attributes.md`, `_tsuzuri/language-reference/organizing-tsuzuri/packages.md`, `_tsuzuri/language-reference/compiler/usage.md` |

## 実装者向け早見

- 範囲: 明示の symbol の `extern "sym" def` の前の属性 `@link("name")`。その extern の symbol を呼ぶ IR を作ったビルドだけ、Tsuzuri がするリンクに `-l<name>` を足す。Windows では、`@link` の付いた symbol の `declare` を `dllimport` にする。どのパッケージ（git・registry を含む）にも書ける。
- 最初に読む: ① 仕様 1〜4 ② `src/driver.rs` の `LinkInputs`・`library_name_error`・`build_complete` のリンク行 ③ `src/llvm_imports.rs` の `host_call` の `declare` ④ `src/driver.rs` の `Project::load_from_root` の `[native]` の規則。
- 最初に書くテスト: `tests/link_attribute.rs` の `link_attribute_parses_and_checks_names` と `dllimport_follows_the_symbol`。
- 触ってはいけない: `[native]`・`-l`・`--link`・`-L` の規則と順序、リンク入力があるときにビルドキャッシュを使わない規則（E12 D7）、`@link` のないプログラムの IR・リンク行・診断、wasm の出力。
- 完了の見分け方: Windows で次のプログラムが `-l gdi32` なしに MSVC と同梱の MinGW の両方でリンクでき、終了コード 7 になる。`--emit llvm` に `declare dllimport i32 @GdiFlush()` が出る。

```tsuzuri
@trusted @link("gdi32")
extern "GdiFlush" def gdi_flush :: unit -> bool

def main :: unit -> i32 = \() ->
    if gdi_flush () then 7 else 8
```

## 目的

extern の宣言は「どの関数を呼ぶか」を書けるが、「どのライブラリにあるか」を書けない。Win32 の宣言を Tsuzuri だけで書いたパッケージ（W27 の `win32` パッケージ）を使うと、利用者は `user32`・`gdi32` などを自分で `-l` に足す必要があり、足し忘れは `E2002` の未定義シンボルになる。git・registry のパッケージは `[native]` を持てないので、パッケージの側で書くこともできない。

このチケットは extern ごとに名前だけのシステムライブラリを書く属性 `@link` を足す。パスは書けないので、パッケージがリンクに自分の目的ファイルを持ち込むことはない。E12 D6 が依存のリンク入力を root の許可（`native = true`）に限った理由は「パスの信頼（供給網の危険）」で、名前だけの `@link` はそれに当たらない（D2）。Windows では、同じ印で `declare` を `dllimport` にし、import の thunk を通らない呼び出しにする。[GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30) の Win32 のアプリを書く道具の 1 つである。

## 着手条件と停止条件

### 着手条件

- `node scripts/repo-kb.mjs brief W17` を読む。決定事項 D1・D2 の承認があること（承認前は手順 2 以降に着手しない）。
- 「現状」の確かめ方を着手時の `main` で実行し、食い違えばこのチケットを直してから始める。
- W14（`declare` の正規化）・W16（`GetLastError` などの補助の `declare`）が先に入っているかを見る。

### 停止条件

- **S1** `dllimport` の `declare` が、MSVC か同梱の MinGW のどちらかでリンクできない、または実行の結果が変わる。
- **S2** `@link` のライブラリを足すのに、`@link` のないビルドのリンク行かキャッシュの鍵を変える必要がある。
- **S3** `--backend fast`（PB05）が `dllimport` の `declare` を、`E2002` でなく別の意味に訳す。
- **S4** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

- **構文**: `@link("user32")` の行の後に extern を書くと `1:1: error[E0002]: expected an expression; '@checked' is the only attribute of an expression`（開発版で確認）。
- **リンクの失敗**: 同梱のツールチェーン（MinGW、`TSUZURI_CLANG=.../tsuzuri-clang.exe`）で `extern "GdiFlush" def gdi_flush :: unit -> bool` を呼ぶ exe を `build` すると、`lld-link: error: undefined symbol: GdiFlush` の `E2002` に、W03 が直す誤った助言「install LLVM/Clang 17+ or set TSUZURI_CLANG to its executable」が付く。`-l gdi32` を足すとリンクでき、終了コード 7。
- **`declare` の形**: `host_call` は `declare {output} @{symbol}(...)`（WASM なら import の属性付き）を `self.intrinsics`（宣言の行の文字列の `BTreeSet`）に入れる。`dllimport` は付かない。上のプログラムの `--emit llvm` は `declare i32 @GdiFlush()`、目的ファイルの未定義シンボルは `GdiFlush`（`llvm-nm` で確認。import library の thunk を通る）。
- **`dllimport` を手で足した確認**: `--emit llvm` の `declare` を `declare dllimport i32 @GdiFlush()` に書き換えて C の `main` とリンクすると、目的ファイルの未定義シンボルは `__imp_GdiFlush` になり、`-lgdi32` 付きで MinGW（zig cc、lld-link）と MSVC（clang 22.1.8 と link.exe 14.44）の両方でリンクでき、終了コード 7。`-lgdi32` がないと `undefined symbol: __declspec(dllimport) GdiFlush`。`dllimport` はリンクに必須ではなく、呼び出しを IAT 経由にして thunk を除く。
- **リンク入力**（`src/driver.rs`）: `LinkInputs` は `paths`・`libraries`・`search`。`-l` の名前は `library_name_error` が検査し（英数字と `_+.-`、`lib` の接頭辞と拡張子を拒む。パスの区切りは通らない）、`check_shape` は重複を `DuplicateLinkInput`、合計 256 超を `LinkInputLimit` にする。`build_complete` はリンク入力が空でないとき、native の `Executable`・`Shared` 以外を `OptionRequiresBuild` にし、ビルドキャッシュを使わない（コメント「The cache key does not cover the contents of link inputs」）。リンク行で `links.add_to` を呼ぶのは 5 か所: `src/driver.rs` の `build_complete` の exe と macOS の dSYM 用の別のリンク、`link_shared`、`src/test_runner.rs` の `build_debug_runner` と `compile_native_runner`（`tsuzuri test`・`bench` の実行ファイル）。ランタイムの Winsock は `#pragma comment(lib, "ws2_32.lib")` に加えて、`build_complete` が `-lws2_32` を足す（コメント「not every linker honors」）。
- **パッケージ**: `src/package.rs` の `parse_manifest` が `[native]`（`link`・`libraries`・`search`）を読む。`Project::load_from_root` は、root と、root が `native = true` を付けた直接の path の依存だけに `[native]` を許し、git・registry のパッケージは `NativeLinkSettingNotAllowed`（`git and registry packages cannot declare [native] link settings; ...`）。`src/fetch.rs` の `publish` も `[native]` を拒む（`tests/packages.rs` の `git_packages_reject_path_dependencies` と `publish_prints_index_entries` が固定）。
- **決定の記録**: E12 D6 は「依存 package のリンク入力は供給網の危険があり、E10 と合わせて決める」。E12 の「追加した拡張と確認」が root の `native = true` による path の依存の opt-in を足した。E15 の権限は、各パッケージの本体が直接参照する extern を `ffi` の使用に数える静的な検査である。
- **`--backend fast`**: `src/backend/module.rs` の `parse_modifiers` は `dllimport` を読み飛ばす（thunk を通る呼び出しになる）。
- **`tsuzuri audit`**: `src/capabilities.rs` の `Report` は extern ごとに名前・symbol・位置・印を文字（`    name -> "symbol"  path:line:col  trusted ...`）と JSON（`{"type":"extern",...,"used":...}`）で出す。ライブラリの欄はない。
- 調査の記録（未再確認）: 同梱の MinGW では、目的ファイルの `-defaultlib:gdi32`（`.drectve`）はリンクを壊し、`-defaultlib:gdi32.lib` は `-lgdi32` と一緒なら通った。MSVC の link.exe は `/DEFAULTLIB` を読む。

確かめ方: `grep -n "pub fn library_name_error\|fn add_to\|links.add_to\|-lws2_32" src/driver.rs`、`grep -n "git and registry packages cannot declare" src/driver.rs`、`grep -n "intrinsics.insert" src/llvm_imports.rs`、`grep -n "\"dllimport\"" src/backend/module.rs`。

## 変えてはいけない意味

- `[native]`・`-l`・`--link`・`-L` の規則、順序、重複の誤り、256 の上限、リンク入力のあるビルドでキャッシュを使わない規則。git・registry のパッケージと publish が `[native]` を拒むこと。
- `@link` のないプログラムの IR（`declare` の形）・リンク行・キャッシュの鍵・診断・`tsuzuri audit` の出力。
- extern の呼び出しの意味（`dllimport` は呼び出しの経路だけを変える）と、wasm の出力。

## 仕様

### 1. 構文

```text
link        = "@" "link" "(" string ")"
declaration = [ doc ] { attribute } [ "private" ] "extern" link_name "def" ident "::" signature
```

- 文字列は 1 つ（エスケープを解いた値）。`@cpu`・`@trusted`・W16 の `@last_error` と任意の順に、明示の symbol を持つ `extern "sym" def`（`extern "module" "sym" def` を含む）の直前に 1 つだけ書ける。`@trusted` と同じ行でもよい。edition に依らない。
- 名前は `-l` と同じ規則（`library_name_error`）: 英数字と `_`・`+`・`.`・`-` だけで、`lib` の接頭辞と拡張子（`.a`・`.lib`・`.dll`・`.so`・`.dylib` など）を持たない。パスは書けない。

### 2. どのビルドに効くか

- **ライブラリの集合**: ビルドの最終の IR が `declare` する symbol ごとに、その symbol を持つ extern の `@link` の名前の和。名前の順（`BTreeSet`）で、`-l` と `[native] libraries` にすでにある名前は除く。IR に現れない symbol（到達しない extern）の名前は足さない。
- **Tsuzuri がリンクするビルド**（`build --emit exe`・`--emit shared`、`run`、native の `test`・`bench`。リンク行は「現状」の 5 か所）: リンク行の利用者のリンク入力（`-L`・パス・`-l`）の後に `-l<name>` を足す。MSVC の経路では Clang が `<name>.lib` にする。CLI・manifest・`@link` の名前の合計は今の 256 の上限に数える（`LinkInputLimit`、今の文）。
- **Tsuzuri がリンクしない出力**（`--emit object`・`llvm`・`bitcode`・`header`）: 目的ファイルに何も埋め込まない。ホストが自分のリンク行に足す（文書と `tsuzuri audit` の一覧で知る。D6）。
- **ビルドキャッシュ**: `@link` はキャッシュを止めない。名前はソースと IR から決まり、両方ともキャッシュの鍵に入っている（`cache::build_key` の `ir-and-embedded-runtime` とソース）。
- **wasm32・wasm64**: `@link` の付いた symbol の呼び出しを生成したら `E2000`（D7）。到達しない宣言は拒否しない。`check` は受理する。

### 3. Windows の `dllimport`

- Windows の target（MSVC・MinGW、x86-64・AArch64）では、`@link` の表にある symbol の `declare` をすべて `declare dllimport ...` にする。表は symbol ごと: 同じ symbol の extern のどれか 1 つに `@link` があれば、その symbol のすべての呼び出しが `dllimport` の `declare` を使う。`@link` のない symbol の `declare` は今のまま。
- W16 の `GetLastError`・`SetLastError` など、生成コードが自分で出す `declare` も同じ表を見る（表にある symbol なら `dllimport`）。同じ symbol に違う 2 行の `declare` を出さない。
- `--backend fast` は `dllimport` を読み飛ばし、thunk を通る呼び出しになる（同じ関数を呼ぶ）。
- Windows 以外では `dllimport` を付けない。

### 4. パッケージと権限

- `@link` は root・path の依存・git・registry のどのパッケージのソースにも書け、`native = true` を要らない。`publish` も拒まない。`[native]` の規則は変えない。
- ライブラリが効くのは、その extern を呼ぶ本体の IR があるときだけで、呼ぶ本体のパッケージは E15 の権限 `ffi` を使う（ポリシーが有効なら許可が要る）。
- `tsuzuri audit` は extern の行にライブラリを出す: 文字は行の末尾に `  link gdi32`、JSON は `"link":"gdi32"` を `used` の後に足す。`@link` のない extern の行は byte 単位で今のまま（`tz-audit/1` に鍵を足すだけ）。

### 5. 診断

| コード | kind（案） | 条件 | メッセージ（案） | 位置 |
| --- | --- | --- | --- | --- |
| E0002 | `link_attribute_syntax` | `@link` の後が `("...")` でない（括弧なし、文字列でない、2 つ以上） | `'@link' takes one library name in parentheses, for example @link("user32")` | 属性 |
| E1008 | `link_invalid_name` | 名前が `library_name_error` に当たる | `invalid library name '{name}' in '@link'; write the name without a path, the 'lib' prefix or an extension, for example @link("user32")` | 文字列 |
| E0002 | `misplaced_link_attribute` | `extern ... def` 以外（`def`・`export def`・`extern type`）の前 | `'@link' applies to an extern function declaration with a link name; remove it or declare the function with extern "symbol"` | 属性 |
| E1008 | `link_needs_symbol` | 明示の symbol のない `extern def` の前 | `'@link' needs an explicit symbol; write extern "symbol" def, because the generated tsuzuri_host_ symbols belong to the host` | 属性 |
| E1008 | `link_repeated` | 1 つの宣言に 2 つ | `'@link' appears twice on one declaration; an extern function comes from one library` | 2 つ目 |
| E2000 | `link_attribute_wasm` | wasm の build で `@link` の symbol の呼び出しを生成した | `'@link("{name}")' asks the native linker for a system library, which wasm output cannot link; build for a native target` | extern の名前 |

## 設計

### 構文と検査

- `src/syntax.rs`: `LinkAttribute { target: Ident, library: String, span: Span, name_span: Span }` と `Program::links: Vec<LinkAttribute>`（`trusted` と同じ脇のリスト）。`src/parser.rs` の `attributes` に `link_attribute`、宣言の種類の判定（`TrustedWrongTarget` を出す所）で `extern`・`def`・明示の symbol を確かめる。`signature` の次の宣言の属性の先読み（`src/parser.rs` の `matches!(name.as_str(), "cpu" | "trusted" | "count")`）に `link` を足す（[M22](M22-attribute-after-bodyless-extern-def.md) が先に入っていれば、先読みは属性の名前を見ないので足さなくてよい）。
- `src/syntax_codec.rs`: 符号化の末尾（その時点の最後の脇のリストの後）に足し、`FRONTEND_FORMAT` を着手時の値から 1 上げる。`src/formatter.rs` の正規化、`src/api_diff.rs` の `Program` の分解（`links: _`。ライブラリは API の型ではない）。
- `src/check.rs`: `HostImport` に `library: Option<String>`。`@trusted` と同じく名前と位置で extern を探す。名前の検査は `driver::library_name_error` を呼ぶ（規則を 2 つにしない）。`CheckedModule` に `link_libraries: BTreeMap<String, BTreeSet<String>>`（symbol → 名前の集合）。

### 生成と `dllimport`

- `src/llvm_imports.rs`: extern の `declare` の行を作る関数を 1 つにし（`host_call` から出す）、`current_platform().windows()` で `module.link_libraries` に symbol があれば `declare dllimport` にする。同じ symbol のすべての呼び出しが同じ文字列を作るので、`intrinsics` の集合で 1 行になる。`unique_declarations` が片方を黙って捨てる形に頼らない。
- wasm: `self.globals.wasm` で `import.library` のある呼び出しを生成したら、`Globals` の別の集合（`abi_native_only` の共有の口の集合とは別。kind が違うため）に関数の番号を記録する。`emit_program` は共有の口と同じ所で、どちらかの集合が空でなければ、両方を合わせて最も小さい番号の extern の名前の位置に、その集合の誤り（`E1008` の `abi_native_only` か `E2000` の `link_attribute_wasm`）を返す。共有の口がまだなければ（W13・W15・W16 のどれも入っていなければ）、このチケットが同じ形で作る。

### リンク行（`src/driver.rs`）

- `system_libraries(module, ir, links) -> Vec<String>`: `RuntimeSymbols::collect(ir)` に含まれる symbol について `module.link_libraries` の名前を集め、`links.libraries` にある名前を除いて名前の順に返す。`build_complete` で 1 回求め、`LinkInputs` とは別に持つ（`LinkInputs` に入れると、`-l user32` と `@link("user32")` の併用が `check_shape` の重複になり、キャッシュも止まる）。
- `LinkInputs::add_to` に `system_libraries: &[String]` の引数を足し、利用者のリンク入力の後に `-l<name>` を足す。引数を足すことで、5 か所の呼び出し（`build_complete` の 2 か所、`link_shared`、`src/test_runner.rs` の 2 か所）とテストの 1 か所のすべてがコンパイルの誤りで見つかり、どれかを足し忘れることがない。テストの実行ファイル（`build_debug_runner`・`compile_native_runner`）は、自分の IR から `system_libraries` を求める。`check_shape` は IR より前に走るので、そこでは数えない。リンク行を組む直前に、リンク入力の数と `system_libraries` の数の合計が 256 を超えたら今の `LinkInputLimit`（今の文）を返す。
- `--emit object`・`llvm`・`bitcode`・`header` は `system_libraries` を使わない。`OptionRequiresBuild` は今どおりリンク入力（CLI・manifest）だけを見る。

### `tsuzuri audit`（`src/capabilities.rs`）

- `ExternReport` に `link: Option<String>`。文字の行の末尾と JSON の鍵（仕様 4）。`link` が `None` の行は今と同じ文字列を作る。

採らなかった案は決定事項 D2・D4・D6 に書く。

## 前提とする他チケットのインターフェース

- なし（単独で出荷できる）。
- W14 が先なら: W14 の `CheckedModule.extern_abis` が symbol ごとに作る 1 行の正規の `declare` に、この表の `dllimport` を付ける（正規化も `dllimport` も symbol ごとなので矛盾しない）。「`declare` の行を作る 1 つの関数」は W14 のその経路を使う。
- W16 が先なら: W16 の補助の `declare`（`GetLastError`・`SetLastError`）も `declare` の行を作る同じ関数を通す。属性の脇のリストは `last_error` の後に `links` を符号化する。
- W13・W15・W16 の `abi_native_only` の共有の口が先にあれば、wasm の拒否はその場所に集合を 1 つ足す。

## 他チケットへ提供するインターフェース

- 属性 `@link("name")`、`HostImport::library`、`CheckedModule::link_libraries`（W27 の生成器が win32metadata の `DllImport("USER32.dll")` を `@link("user32")` に写す）。
- extern の `declare` の行を作る 1 つの関数（W14 の正規化と W16 の補助が使う）。
- `tsuzuri audit` の `link`（W27 の公式パッケージの監査）。

## 実装手順

### 手順 1: ベースライン

- 変更: なし（`target/scratch/w17/` の下書きだけ）。
- 内容: `node scripts/repo-kb.mjs brief W17`。`tests/fixtures/*` と `examples/*` の `Main.tz` の `--emit llvm`（native と wasm32）と、`tsuzuri audit --json` のある fixture（`tests/capabilities.rs` のコーパス）の出力を保存する。`FRONTEND_FORMAT` の今の値を控える。
- 確認: 保存した件数を記録する。

### 手順 2: 構文と検査

- 変更: `src/syntax.rs`、`src/parser.rs`、`src/syntax_codec.rs`、`src/frontend_cache.rs`、`src/formatter.rs`、`src/api_diff.rs`、`src/check.rs`、`src/diagnostic_kinds.rs`。
- 内容: 設計の「構文と検査」。仕様 1 と仕様 5 の `E0002`・`E1008`、`HostImport::library` と `CheckedModule::link_libraries`。
- 確認: T0。`link_attribute_parses_and_checks_names`。`tsuzuri fmt` が `@link` を保つ。T1。

### 手順 3: `dllimport` と wasm

- 変更: `src/llvm_imports.rs`、`src/llvm.rs`。
- 内容: 仕様 3 の `dllimport`（`declare` の行を作る 1 つの関数）と、仕様 2 の wasm の `E2000`。
- 確認: `dllimport_follows_the_symbol`・`wasm_rejects_reached_link_externs`。手順 1 の IR と、この機能を使わない入力の IR が `cmp` で一致する。T1。

### 手順 4: リンク行

- 変更: `src/driver.rs`（`system_libraries`、`add_to` の引数、上限）、`src/test_runner.rs`（2 か所）。
- 内容: 設計の「リンク行」。到達した symbol のライブラリだけを、利用者のリンク入力の後に足す。
- 確認: `driver::tests` の `link_attribute_libraries_reach_the_msvc_linker`、`tests/link_attribute.rs` の `only_reached_symbols_add_libraries`・`link_attribute_keeps_the_build_cache`。T1。

### 手順 5: パッケージと audit

- 変更: `src/capabilities.rs`、`tests/packages.rs`、`tests/capabilities.rs`。
- 内容: 仕様 4 の audit の `link` と、git・registry のパッケージの `@link` の受理（`[native]` の規則は変えない）。
- 確認: `git_packages_may_name_system_libraries`、audit の出力の検査。手順 1 の audit の出力が byte 単位で一致する。

### 手順 6: E2E と文書

- 変更: `tests/ffi_extensions.mjs`、`tests/windows.mjs`、文書。
- 内容: 「テスト計画」の E2E と「ドキュメント」の節。`node scripts/gen-docs.mjs` で診断の表を作り直す。
- 確認: 「テスト計画」の E2E。T2。PR に `ci:full` を付ける（Windows のリンクを変える）。

## テスト計画

### Rust テスト

- `tests/link_attribute.rs`（新規）:
  - `link_attribute_parses_and_checks_names`: `@link("user32")`・`@trusted @link("gdi32")` の 1 行と 2 行・`extern "env" "f" def` の前を受理。`@link`・`@link(user32)`・`@link("a", "b")` は `E0002`。`"libuser32"`・`"user32.lib"`・`"../x"`・`"C:/x"`・`""` は `E1008` の `link_invalid_name`。`def`・`export def`・`extern type` の前は `E0002` の `misplaced_link_attribute`、明示の symbol のない `extern def` は `link_needs_symbol`、重複は `link_repeated`。本体のない extern の次の宣言の `@link` が制約行と読まれない。
  - `dllimport_follows_the_symbol`: `PlatformScope::enter` で `x86_64-pc-windows-msvc`・`aarch64-pc-windows-gnu` を選ぶと、`@link` の symbol は `declare dllimport`、`@link` のない symbol は今の形。同じ symbol を `@link` の有無で 2 回宣言しても `declare` は `dllimport` の 1 行。`x86_64-unknown-linux-gnu` では `dllimport` なし。IR は 2 回の生成で同じ。
  - `wasm_rejects_reached_link_externs`: `llvm::emit_target(.., true)` が到達する extern で `E2000` の `link_attribute_wasm`、到達しないだけなら成功。
  - `only_reached_symbols_add_libraries`: 呼ばない extern の `@link("no_such_library")` は `system_libraries` に入らない。`-l gdi32` と `@link("gdi32")` の併用で名前は 1 回、`E2000` の重複にならない。
  - `link_attribute_keeps_the_build_cache`: `tests/targets.rs` の `a_different_triple_or_sysroot_never_reuses_a_cached_build` と同じく、呼び出しを記録する偽の clang で、`@link` のあるビルドの 2 回目に clang が走らない。
- `src/driver.rs` の単体テスト `link_attribute_libraries_reach_the_msvc_linker`: `link_inputs_reach_the_msvc_linker_as_libpath_and_lib_names` と同じ `clang -###` の dry run で、`-luser32` が x86_64・aarch64 の `windows-msvc` の linker の行に `user32.lib` として出る（Clang がなければ何も検査しない）。
- `tests/packages.rs` の `git_packages_may_name_system_libraries`: `offline_project` の git パッケージの extern に `@link("m")` を書いて `check` が成功する。同じパッケージの `[native]` は今どおり `E2000`（既存のテストは変えない）。
- `tests/capabilities.rs`: audit の文字と JSON に `link` が出る。`@link` のない extern の行は今の文字列のまま。

### E2E

- `tests/ffi_extensions.mjs`（POSIX）: テストの中で `ar` で `libw17.a`（`int64_t w17_answer(void)` が 42）を作り、`@link("w17") extern "w17_answer"` を呼ぶプログラムが `-L <dir>` だけで `build` でき 42 を返す。`@link` を消すと `E2002`。`native = true` のない path の依存の `@link` でも同じく動く（同じ依存の `[native]` は今どおり `E2000`）。
- `tests/windows.mjs`: 早見のプログラムが `-l gdi32` なしに MSVC と同梱の MinGW でリンクでき 7。`--emit llvm` に `declare dllimport i32 @GdiFlush()`。`llvm-nm` があれば `--emit object` の未定義シンボルが `__imp_GdiFlush`。`--backend fast` でも 7。
- wasm32・wasm64 の `build` は到達した `@link` の extern で `E2000`、`check` は成功する。

### 比較

- 手順 1 の基準と、この機能を使わない全入力の `--emit llvm`（native・wasm32）と audit の出力が byte 単位で一致する。

## ドキュメント

- `_tsuzuri/language-reference/compiler/native-interop.md` の「リンク入力」に小節「extern ごとのライブラリ（@link）」: 書き方、到達したときだけ足すこと、Windows の `dllimport`、`--emit object`・`llvm` ではホストが `-l` を足すこと、`[native]` との使い分け（パスと静的ライブラリの順序は `[native]`）。
- `_tsuzuri/language-reference/values-and-functions/attributes.md` の「属性の一覧と適用対象」の表と、新しい節「@link（extern のシステムライブラリ）」（書ける位置とエラー）。
- `_tsuzuri/language-reference/organizing-tsuzuri/packages.md` の `[native]` の節: git・registry のパッケージは `[native]` を持てないが `@link` は書けること、`ffi` の権限との関係。
- `_tsuzuri/language-reference/compiler/usage.md` の audit の出力の例と JSON の鍵。
- `docs/language.md` の「リンク名」「リンク入力」。`compiler/diagnostics.md` の生成された表（`node scripts/gen-docs.mjs`）。`node scripts/check-docs.mjs` を変えたページに通す（Windows 専用の例は `run=` を付けない）。

## 受け入れ条件

- [ ] 仕様 1〜5 のとおりに動く（MSVC・MinGW・Linux・macOS、`-O0`・`-O3`）。
- [ ] 到達しない extern の `@link` はリンク行に出ず、`-l` との併用で重複の誤りにならない。
- [ ] Windows で `@link` の symbol の `declare` がすべて `dllimport` の 1 行で、`@link` のない symbol は変わらない。
- [ ] `@link` があってもビルドキャッシュが効く。リンク入力のある既存のビルドは今どおりキャッシュを使わない。
- [ ] git・registry のパッケージの `@link` が受理され、`[native]` の規則と publish の拒否は変わらない。
- [ ] `@link` を使わないプログラムの IR・object・header・リンク行・診断・audit の出力が byte 単位で変わらない（比べた件数を記録した）。
- [ ] 文書を直し、T2 が成功し、PR に `ci:full` を付けた。

## 落とし穴

- `signature` の先読みに `link` を足し忘れると、本体のない extern の直後の宣言の `@link` が前の宣言の制約行として読まれる（同じ種類の不具合が `@c_layout`・`@literal` で起きている。[M22](M22-attribute-after-bodyless-extern-def.md)）。
- `@link` の名前を `LinkInputs::libraries` に入れない。重複の誤り（`-l` との併用）とキャッシュの停止（E12 D7）を引き起こす。
- `dllimport` を `host_call` の呼び出しごとに決めない。同じ symbol に `dllimport` の有無の違う 2 行ができ、`unique_declarations` が先の行だけを残す（どちらが残るかは文字列の順で決まる）。
- `dllimport` を `@link` のない明示の symbol に広げない。静的ライブラリやホストの目的ファイルが定義する関数への `dllimport` の参照は、リンカーの警告（局所の定義の import）や失敗になりうる。
- ライブラリの集合を IR の文字列から求めるときは、`RuntimeSymbols` と同じくコメントと文字列の中の `@` を数えない。
- Windows のリンク行に `-defaultlib:` を足さない（同梱の MinGW で `.drectve` の `-defaultlib:gdi32` がリンクを壊したという調査の記録がある）。

## 対象外

- パスの `@link`、静的ライブラリの順序の指定、macOS の framework（`-framework`）。パスと順序は `[native]` を使う。framework は [M20](M20-macos-framework-link-inputs.md) が `--framework`・`[native] frameworks` として足し、その Phase 2 が W17 の出荷の後に `@link(framework = "...")` へ広げる（D-67。M20 の D6 で要承認）。
- `--emit object`・`llvm`・`bitcode` の目的ファイルへのライブラリの要求の埋め込み（COFF の `/DEFAULTLIB`、ELF の `.deplibs`、Mach-O の `LC_LINKER_OPTION`。D6）。
- `[native]` の規則の緩和（git・registry のパッケージのパスのリンク入力）。
- 遅延読み込み（`/DELAYLOAD`）と `GetProcAddress` による実行時の解決。
- `tsuzuri bindgen` が `@link` を書くこと（W27 の生成器が書く）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 明示の symbol の extern の前の属性 `@link("name")` を足す（D-53 の閉じた属性の集合の改訂）。名前は `-l` と同じ規則でパスを持たない | ライブラリは関数ごとの事実（win32metadata の `DllImport` も関数ごと）で、宣言の隣に書くのが最も取り違えにくい。明示の symbol に限るのは、`tsuzuri_host_` の symbol はホストが定義する契約だから | 要承認（承認前は手順 2 以降に着手しない） |
| D2 | `@link` は git・registry を含むどのパッケージにも書け、`native = true` を要らない（E12 D6 の拡張） | E12 D6 の制限の理由は、依存が持ち込むパスの目的ファイル（供給網の危険）。名前だけの `@link` はパッケージのファイルをリンクに入れず、システムか root が指定した探索のディレクトリにあるライブラリを選ぶだけで、効くのはその extern を呼ぶ本体（E15 の `ffi` の使用）があるときだけ。どのパッケージが何を要るかは `tsuzuri audit` に出る（D8）。採らなかった案: `ffi` を許した依存にだけ `[native] libraries` を許す（manifest の規則が 2 種類になり、関数との対応も失う） | 要承認（承認前は手順 5 に着手しない） |
| D3 | ライブラリは、最終の IR が `declare` する symbol の `@link` の和で決め、名前の順に、利用者のリンク入力の後に足す | 「到達したときだけ」を、出力の種類ごとの根の選び方を作り直さずに決められる（IR に現れる symbol は生成した呼び出しだけ）。システムの共有ライブラリは順序に依らない | 既定案（実装者はこの案に従う） |
| D4 | Windows では、`@link` の表にある symbol の `declare` だけを、symbol ごとに `dllimport` にする | `@link` は DLL の import library を名指すので、IAT を直接読む呼び出しで thunk を除ける。どちらの形も MSVC・MinGW でリンクと実行ができることを確認した（意味は変わらない）。性能の数字は計測していないので主張しない | 既定案（実装者はこの案に従う） |
| D5 | `@link` の名前は `LinkInputs` と別に持ち、`-l` と重複したら黙って 1 つにし、256 の上限には数える。ビルドキャッシュは止めない | E12 D7 がキャッシュを止める理由は「鍵が入力のファイルの内容を知らない」で、`@link` の名前はソースと IR に入っていて鍵が覆う。同じ名前を 2 か所に書くのは誤りでない | 既定案（実装者はこの案に従う） |
| D6 | Tsuzuri がリンクしない出力（object・llvm・bitcode・header）には何も埋め込まない | 目的ファイルの要求は形式ごとに効き方が違う（COFF の `/DEFAULTLIB` は link.exe が読むが同梱の MinGW では壊れたという記録、ELF の `.deplibs` は lld だけ、Mach-O は macOS だけ）。半端に埋めるとホストのリンクの結果がツールチェーンで変わる。ホストは文書と audit の一覧で `-l` を足す | 既定案（実装者はこの案に従う） |
| D7 | wasm32・wasm64 は、`@link` の symbol の呼び出しを生成したときだけ `E2000` | 共通契約のとおり。wasm にはシステムライブラリのリンクがない。到達だけで拒否すれば、Windows 向けの宣言を持つパッケージを wasm のプログラムが読み込める | 既定案（実装者はこの案に従う） |
| D8 | `tsuzuri audit` の extern の行に `link` を出す（`tz-audit/1` に鍵を足す。値のない行は不変） | D2 で依存がライブラリを要求できるようになるので、レビューの対象を見えるようにする | 既定案（実装者はこの案に従う） |
| D9 | 誤りは `E0002`・`E1008`・`E2000` の新しい kind で表す | 新しいコードを割り当てない（E12 D12・D-51）。構文の誤りと属性の置き場の誤り（`misplaced_link_attribute`。`@cpu` の `misplaced_cpu_attribute` と同じ。W13・W16・W21 とそろえる）は `E0002`、宣言の誤りは ABI の `E1008`、target の誤りは `E2000` の分類 | 既定案（実装者はこの案に従う） |
