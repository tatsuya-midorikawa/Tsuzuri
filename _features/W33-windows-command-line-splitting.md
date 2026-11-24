# W33: Windows のコマンドラインの分割を CRT（`CommandLineToArgvW`）の規則にそろえる

| 項目 | 内容 |
| --- | --- |
| ID | W33 |
| 優先度 | P2（`tsuzuri script` の「引数をそのままプログラムへ渡す」が Windows で成り立たない。`"` を含む引数と、空白を含みバックスラッシュで終わる引数が、別の個数・別の文字で届く） |
| 規模 | M（ランタイムの C の分割 1 つ、手書きの表のテスト、Windows の CRT との差分テスト、往復のテスト、文書 6 ページ） |
| 依存 | – |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。W01〜W30 の起票中に見つけ、利用者の依頼「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」に当たるので起票。HEAD `fa40eede` で確認） |
| 承認 | 要承認（利用者に見える引数の変化、`env.md`・`docs/language.md` の契約の改訂。`"C:\dir\"` が `C:\dir\` でなく `C:\dir"` になる。決定事項 D1・D3・D4） |
| 改善する劣位 | Windows の Tsuzuri のプログラムは、リテラルの `"` を引数として受け取れず、Rust の `Command`・Node の `spawnSync`・`tsuzuri script` 自身・[W11](W11-windows-os-api.md) の `Process.run`（D9）が CRT の規則で引用した引数を、別の個数・別の文字で受け取る。C・C++・Rust のプログラムと同じコマンドラインから違う配列を得る |
| 手本にする既存実装 | `src/runtime/arguments.c` の `tz_arguments_split`（2 回の走査: 数えてから埋める）、`tests/arguments_runtime.c` の `SPLIT` の表、`tests/arguments.rs` の `executables_receive_their_command_line_arguments`、`tests/windows.mjs` の C の補助のコンパイル（`nativeFlags`） |
| 主な影響ファイル | `src/runtime/arguments.c`（W11 が先なら `src/runtime/arguments-split.h`）、`tests/arguments_runtime.c`、`tests/arguments.rs`、`tests/arguments_reference.c`（新規）、`tests/windows.mjs`、`tests/script.mjs`、`_tsuzuri/language-reference/built-in-types-and-modules/env.md`、`_tsuzuri/language-reference/compiler/usage.md`、`_tsuzuri/language-reference/organizing-tsuzuri/modules.md`（確認のみ）、`docs/language.md`、`docs/architecture.md`、`docs/stability.md` |

## 実装者向け早見

- 範囲: Windows の `def main :: Array<string> -> i32` の引数（`tsuzuri_arguments` が `GetCommandLineW()` を分ける）を、UCRT（Visual Studio 2008 以降の CRT）が `wmain` に渡す `argv[1..]` とまったく同じにする。題の「CRT（`CommandLineToArgvW`）」は起票時の表記で、規則の正は UCRT である。`CommandLineToArgvW` とは 3 つの場合で違う（「現状」の表、D1）。POSIX と WASI は変えない。生成 IR は変えない。
- 最初に読む: 「現状」の表 → `src/runtime/arguments.c` の `tz_arguments_blank`・`tz_arguments_token`・`tz_arguments_split` → 「仕様」の規則 → 「落とし穴」のバッチファイル。
- 最初に書くテスト: `tests/arguments_runtime.c` の表（「テスト計画」1。期待値は規則から手で書く）。
- 触ってはいけない: `tz_arguments_from_utf8`・`tz_arguments_decode`（POSIX と WASI）、`#if !defined(_WIN32) || defined(TZ_ARGUMENTS_TEST)` の区切り（POSIX の機械でも Windows の分割を試験する仕組み）、`tsuzuri_arguments` の型と、`main` の入口の IR。
- 完了の見分け方: Git Bash で次を実行すると、標準出力が `[a"b][C:\dir with space\][x y][][C:\plain\]` で、`E2005` の文が `program exited with code 5` になる（今は `[a\b C:\dir][with][space\\ x][y  C:\plain\]` と `code 4`）。

```sh
T=/c/Users/tmidorikawa/.vscode/extensions/tmidorikawa.tsuzuri-0.1.6-win32-x64/toolchain
mkdir -p target/scratch/w33
cat > target/scratch/w33/script.tz <<'EOF'
def main :: Array<string> -> i32 = \args ->
    let separator = "]["
    do! IO.writeln ("[" + String.join (ref separator) (ref args) + "]")
    args.length as i32
EOF
TSUZURI_CACHE_DIR= TSUZURI_CLANG=$T/bin/tsuzuri-clang.exe target/debug/tsuzuri.exe script target/scratch/w33/script.tz 'a"b' 'C:\dir with space\' 'x y' '' 'C:\plain\'
```

## 目的

Windows のプロセスは 1 本のコマンドラインを受け取り、分け方はプログラムの CRT が決める。Windows の引用の書き手（Rust の `std::process::Command`、Node の `child_process`、`tsuzuri script`、W11 の `Process.run`）は、どれも MSVC の CRT が元に戻せる形で引用する。Tsuzuri のプログラムだけが独自の規則（バックスラッシュを特別扱いせず、`"` は引用の状態を反転するだけ）で分けるので、`"` を含む引数を受け取れず、空白を含みバックスラッシュで終わる引数は文字が変わり、後ろの引数まで巻き込む。このチケットは、Tsuzuri のプログラムが C のプログラムと同じ配列を受け取るように、分割を UCRT の規則にそろえる。

## 着手条件と停止条件

### 着手条件

- **人間の承認**: 決定事項 D1・D3・D4（規則の変更、先頭の空白、edition に結び付けないこと）。承認前は手順 2 以降に着手しない（手順 1 の基準の保存はしてよい）。承認者に見せるもの: 「現状」の表と「落とし穴」のバッチファイルの例（`"C:\dir\"` が `C:\dir"` になり、後ろの引数を巻き込むこと）。
- `node scripts/repo-kb.mjs brief W33`。`gh run list --branch main --workflow ci.yml --limit 1` と `windows.yml` の main の最新の結果を基準にする。
- [W11](W11-windows-os-api.md) の状態を確かめる。W11 の手順 4 が入っていれば、分割は `src/runtime/arguments-split.h` にあり、`Env.args` も同じ関数を使う。このチケットはその 1 か所を変える（「設計」4）。
- Windows の開発機（MSVC の Developer PowerShell と LLVM の clang、または同梱の `tsuzuri-clang`）。差分テスト（テスト計画 3）は Windows でしか走らない。macOS・Linux だけの開発機では、表のテスト（テスト計画 1）は走るが、差分テストは `ci:full` の `windows.yml` に頼る。

### 停止条件

- **S1** 承認が得られない（D1・D3・D4）。
- **S2** どれかのプログラムの `--emit llvm`（`tests/fixtures`・`examples` の `Main.tz`、native・wasm32・`--target x86_64-pc-windows-msvc`、`-O0`・`-O3`）が 1 byte でも変わる。このチケットはランタイムの C だけを変える前提で書いてある。
- **S3** 差分テスト（テスト計画 3）で、CI の MSVC の clang で作った参照のプログラムの `argv` が、このチケットの表（UCRT の規則から手で書いたもの）と食い違う行がある。表を写し直して合わせず、行と両方の結果を報告する（UCRT の版の違いの可能性）。
- **S4** POSIX か WASI の引数（`tests/arguments.rs` の POSIX の検査、`tests/os.mjs` の 14b）の結果が変わる。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

### 実装と契約

- `src/runtime/arguments.c` の `tsuzuri_arguments`（`#if defined(_WIN32)` の枝）は `count`・`values` を使わず、`tz_arguments_split((const uint16_t *)GetCommandLineW(), output)` を呼ぶ。
- `tz_arguments_split` は、先頭の空白とタブを飛ばし（`tz_arguments_blank`）、最初の語をプログラム名として `tz_arguments_token` で読み飛ばし、残りを空白・タブで分ける。`tz_arguments_token` は `"` で `quoted` を反転して `"` を落とし、ほかの文字はそのまま写す。**バックスラッシュに特別な意味はなく、リテラルの `"` を表す方法がない。** ファイルの先頭の注釈も「backslashes have no special meaning」と書く。
- この規則は契約として書かれている:
  - `_tsuzuri/language-reference/built-in-types-and-modules/env.md` の「引数と main」: 「Windows ではプロセスが 1 本のコマンドラインを受け取るので、実行ファイル側が空白と引用符で分けます。バックスラッシュに特別なエスケープはありません。」と「ビルドした実行ファイルに引数を渡すと、両方に同じ個数が入ります」（`Env.args` と `main` の引数）。
  - `docs/language.md` の「アプリケーションのエントリーポイント」: 「バックスラッシュ記号 `\` に特別なエスケープ意味論は持たせず、例えば `"C:\dir\"` は文字列 `C:\dir\` として扱われます」。
  - `docs/architecture.md` の「`def main` の入口」: 「バックスラッシュは特別扱いされません」。
  - 規則は 2291223a（2026-10-05、`def main :: Array<string> -> i32` の導入）で入り、6787f050（2026-10-06）で説明だけを直した（「実装はもともとこの規則なので変えず」）。CRT の規則と比べて決めた記録は、コミットにも完了済みのチケットにも見つからなかった。
- 一方、`_tsuzuri/language-reference/compiler/usage.md` の「script」は「ファイルより後ろの引数は、`--help` のようにオプションに見えるものも含めてそのままプログラムへ渡り」と約束する。`src/driver.rs` の `run_process` は、`program_command` が返す Rust の `Command` に `.args(arguments)` を足して起動するので、Windows では Rust の標準ライブラリが CRT の規則で引用したコマンドラインになる（再現 2 で、Rust の `Command` の引用を C のプログラムが元のとおりに受け取ることを確かめた）。
- 生成 IR: `def main :: Array<string> -> i32` の `@main(i32 %argc, ptr %argv)` が `declare void @tsuzuri_arguments(i32, ptr, ptr)` を呼ぶ（`docs/architecture.md`）。driver は IR が `@tsuzuri_arguments(` を含むときだけ `arguments.c` を連結する（`src/driver.rs` の `arguments_runtime`）。分割を変えても IR は変わらない。
- ランタイムの事前ビルド（`src/runtime_cache.rs` の `prebuilt`）は、単位の C の本文で照合する。`arguments.c` を変えると、古い配布物の事前ビルドは外れて Clang でコンパイルし直す（結果は正しい）。

### 再現（開発版 `target/debug/tsuzuri.exe`、Windows 11 Enterprise 10.0.26300、x64）

1. `tsuzuri script`（「実装者向け早見」のコマンド）: 5 つの引数を渡すと、プログラムは 4 つの引数 `[a\b C:\dir][with][space\\ x][y  C:\plain\]` を受け取り、`script.tz:1:1: error[E2005]: program exited with code 4`。同じ引数を Git Bash から C のプログラム（下の `probe`）に渡すと、5 つがそのまま届く。
2. Rust の `Command::new(p).args([...])` と Node の `spawnSync(p, [...])` の両方で、`a"b`・`C:\dir with space\`・`x y`・空・`C:\plain\`・`""`・`a\\"b c`・`tab<TAB>here`・`日本` の 9 つを渡すと、C のプログラムの CRT の `argv` と `CommandLineToArgvW` はどちらも 9 つを元のとおりに受け取り、Tsuzuri のプログラムは 6 つの壊れた引数を受け取った（終了コード 6）。
3. 生のコマンドラインを `CreateProcessW` にそのまま渡す起動器で、3 つの受け手を比べた。`probe` は `wmain` の `argv` と `CommandLineToArgvW(GetCommandLineW())` を出す C のプログラムで、同梱の `tsuzuri-clang`（`-municode`、UCRT の DLL）と MSVC の `cl`（静的な UCRT）の 2 通りに作った。**2 つの CRT は 30 行のすべてで同じだった。** Tsuzuri の列は「早見」の `script.tz` と同じ本文を `build` した実行ファイル（同梱の MinGW。UCRT の DLL を import するが、`argv` は使わない）。

| コマンドライン（生） | UCRT の `argv[1..]` | `CommandLineToArgvW` の `[1..]` | 今の Tsuzuri |
| --- | --- | --- | --- |
| `app a\"b` | `a"b` | 同じ | `a\b` |
| `app "a\" b"` | `a" b` | 同じ | `a\`・`b` |
| `app "C:\dir\"` | `C:\dir"` | 同じ | `C:\dir\` |
| `app "C:\dir\\"` | `C:\dir\` | 同じ | `C:\dir\\` |
| `app C:\dir\ x` | `C:\dir\`・`x` | 同じ | 同じ |
| `app a\\\\"b c"` | `a\\b c` | 同じ | `a\\\\b c` |
| `app "a\\\"b c"` | `a\"b c` | 同じ | `a\\\b`・`c` |
| `app \\"` | `\` | 同じ | `\\` |
| `app "a""b"` | `a"b` | 同じ | `ab` |
| `app """"` | `"` | 同じ | 空 |
| `app "x""y"z"` | `x"yz` | 同じ | `xyz` |
| `app "a"" b"` | `a" b` | `a"`・`b` | `a b` |
| `app """""` | `""` | `"` | 空 |
| `app a "" b` | `a`・空・`b` | 同じ | 同じ |
| `app "unterminated b` | `unterminated b` | 同じ | 同じ |
| `app a<LF>b`（改行） | `a<LF>b`（1 つ） | 同じ | 同じ |
| `␠␠app x`（先頭が空白） | `app`・`x`（プログラム名は空） | 同じ | `x` |
| `C:\dir\"x y" z` | `z`（プログラム名は `C:\dir\x y`） | `y z`（プログラム名は `C:\dir\"x`） | `z` |

- UCRT と `CommandLineToArgvW` は、**3 つの場合で違う**: 引用の中の `""`（UCRT はリテラルの `"` で引用を続け、`CommandLineToArgvW` はリテラルの `"` で引用を閉じる）が 2 行、`"` で始まらないプログラム名の中の `"`（UCRT は引用を反転し、`CommandLineToArgvW` は最初の空白で切る）が 1 行。どちらも、上の引用の書き手は出さない形である（引用の中の `"` は `\"` と書く）。
- プログラム名の読み方（`"` を反転して落とし、バックスラッシュはそのまま、引用の外の空白・タブで終わる）は、今の Tsuzuri と UCRT で同じ（`C:\dir\"x y" z` の行）。違うのは先頭の空白を飛ばすことだけ。

## 仕様

Windows の `def main :: Array<string> -> i32` は、`GetCommandLineW()` を次の規則で分け、プログラム名の後ろの引数を受け取る。これは UCRT（Visual Studio 2008 以降）が `wmain` に渡す `argv[1..]` と同じである。

1. **プログラム名**: コマンドラインの最初の文字から読む（先頭の空白・タブを飛ばさない）。`"` は引用の状態を反転して取り除き、バックスラッシュは普通の文字、引用の外の空白・タブで終わる。コマンドラインが空白かタブで始まれば、プログラム名は空で、最初の語は引数になる。
2. **区切り**: 引数と引数の間の空白（U+0020）とタブ（U+0009）は、いくつ続いても 1 つの区切り。改行などほかの空白類は普通の文字。
3. **引数の中**:
   - `"` の前の 2n 個のバックスラッシュは n 個のバックスラッシュになり、その `"` は引用を開くか閉じる（`"` は残らない）。
   - `"` の前の 2n + 1 個のバックスラッシュは n 個のバックスラッシュとリテラルの `"` になる（引用の状態は変わらない）。
   - `"` の前でないバックスラッシュは、そのまま残る（`C:\dir\ x` の `C:\dir\`、`\\server\share`）。
   - 引用の中で `""` が続くと、リテラルの `"` 1 つになり、引用は続く（`"a""b"` は `a"b`）。
   - 引用の外の空白・タブか、コマンドラインの終わりで引数が終わる。閉じない引用は終わりまで続く。
4. 各引数は UTF-16 のまま `string` になる（今と同じ。孤立サロゲートもそのまま）。
5. 変えないもの: POSIX と WASI の引数（OS とホストが分けたもの）、`tsuzuri run` が引数を渡さないこと、ライブラリ・既定の WASM の空の配列、生成 IR、`Env.args`（W11 の後は同じ関数を使うので、同じ規則に自動で従う）。

## 設計

### 1. 分割（`src/runtime/arguments.c`）

`tz_arguments_token` を UCRT の規則に書き換え、プログラム名を読む関数を分ける。`tz_arguments_blank` と 2 回の走査（数えてから埋める）はそのまま使う。

```c
/* The program name (UCRT): a quote toggles quoting and is removed, a backslash is an ordinary
 * character, and a space or a tab outside quotes ends it. A line that begins with a blank has an
 * empty program name, so its first word is an argument. */
static const uint16_t *tz_arguments_program(const uint16_t *at) {
    int quoted = 0;
    for (; *at != 0 && (quoted || (*at != ' ' && *at != '\t')); at++) {
        if (*at == '"') quoted = !quoted;
    }
    return at;
}

/* One argument (UCRT, Visual Studio 2008 and later): 2n backslashes and a quote are n backslashes
 * and a quote that opens or closes quoting; 2n + 1 backslashes and a quote are n backslashes and a
 * literal quote; other backslashes are literal; inside quotes, "" is one literal quote. */
static const uint16_t *tz_arguments_token(const uint16_t *at, uint16_t *output, int64_t *length) {
    int quoted = 0;
    int64_t count = 0;
    for (;;) {
        int64_t slashes = 0;
        while (*at == '\\') {
            at++;
            slashes++;
        }
        int copy = 1;
        if (*at == '"') {
            if (slashes % 2 == 0) {
                if (quoted && at[1] == '"') {
                    at++; /* "" inside quotes: the second is a literal quote */
                } else {
                    copy = 0;
                    quoted = !quoted;
                }
            }
            slashes /= 2;
        }
        for (; slashes > 0; slashes--) {
            if (output) output[count] = '\\';
            count++;
        }
        if (*at == 0 || (!quoted && (*at == ' ' || *at == '\t'))) break;
        if (copy) {
            if (output) output[count] = *at;
            count++;
        }
        at++;
    }
    *length = count;
    return at;
}
```

- `tz_arguments_split` の先頭は `const uint16_t *start = tz_arguments_blank(tz_arguments_program(line));` にする（今の「先頭の空白を飛ばし、最初の語を `tz_arguments_token` で読む」を置き換える）。後の数える走査と埋める走査は今と同じ。
- 書き出す単位の数は入力の単位の数を超えない（`""` と `\"` は 2 単位から 1 単位、`\\"` は 3 単位から 1 単位）。1 回目の走査が数えた長さで確保し、2 回目は同じ関数で埋める。
- ファイルの先頭の注釈を、「Windows は UCRT と同じ規則で分ける（規則の要約）」に直す。

### 2. 実装の方式（D2）

- 自前の C で書く。`CommandLineToArgvW` は「現状」の 3 つの場合で UCRT と違い、`shell32.dll` の読み込みとリンク（`-lshell32`）が要る。CRT の `__wargv` は、ナローの `main` の入口（Tsuzuri の `@main(i32, ptr)`）では埋まらない（起票時に、ナローの `main` で `__wargv` を出す C を同梱の `tsuzuri-clang` と MSVC の `cl` でビルドすると、どちらも NULL だった）。埋めるには CRT の起動の内部の関数（UCRT の `_configure_wide_argv` など。試していない）に頼ることになる。どちらも、`tests/arguments_runtime.c` が POSIX の機械で分割を試験する今の仕組み（`TZ_ARGUMENTS_TEST`）を失う。
- 自前の実装の正しさは、Windows の CI で本物の UCRT を参照にして確かめる（テスト計画 3）。

### 3. `Env.args` との関係（D5）

- 今の HEAD の Windows では `Env.args` に到達すると `E2002`（OS API が Windows にない）。W11 の D4 は、Windows の `Env.args` を `arguments.c` と同じ分割で求めると決めている（`arguments-split.h` を共有）。分割の実装は 1 つだけなので、このチケットの規則は `Env.args` にもそのまま効き、「`main` の引数と `Env.args` は同じ個数」（`env.md`）は保たれる。
- W11 の D4 の理由の文（「バックスラッシュは特別でない」を約束しているので `CommandLineToArgvW` を採らない）は、このチケットの後は「UCRT の規則を 1 つの C の実装で持つ。`CommandLineToArgvW` は 3 つの場合で違い `shell32` が要る」に読み替わる。D4 の決定（`CommandLineToArgvW` を採らない、`arguments.c` と同じ関数）は変わらない。

### 4. W11 との順序

- **このチケットが先**: `arguments.c` を変える。W11 は手順 4 で `tz_arguments_blank`・`tz_arguments_program`・`tz_arguments_token` を `arguments-split.h` へ移す（W11 は「振る舞いは変えない（移動だけ）」なので、移すのはこのチケットの新しい規則）。W11 の 9 群のテスト（`"C:\dir\"`・`a"b` の期待値）と、W11 の仕様 4 の「Tsuzuri の子への注意」は、W11 の実装者が新しい規則に合わせる（注意は不要になり消える）。
- **W11 が先**: `src/runtime/arguments-split.h` の同じ関数を変える。W11 の 9 群の期待値と、文書に入った「Tsuzuri の子への注意」を、このチケットの PR で直す。
- どちらの順でも、分割の C は 1 か所だけにする（写しを作らない）。

### 5. 互換の扱い（D4・D6）

- edition に結び付けない。ランタイムの C は edition を知らず（実行ファイルは複数の edition のパッケージから作られる）、`main` の引数と `Env.args` は同じ規則でなければならない。edition で分けると、2026 のパッケージのプログラムは壊れた往復を持ち続ける。
- 旧い規則に戻す切り替え（環境変数・フラグ）は設けない。新しい環境変数とフラグは D-55 の登録になり、2 つの規則を持ち続けることになる。旧い規則に頼っていたコマンドラインは、C のプログラムと同じ書き方に直せば同じ配列になる（「落とし穴」）。
- `docs/stability.md` に、edition と無関係に変えた実行時の規則として記録する（「ドキュメント」）。

## 前提とする他チケットのインターフェース

- なし。[W11](W11-windows-os-api.md) が先に入っていれば、その `src/runtime/arguments-split.h`（Windows のコマンドラインの分割の唯一の実装）を変える（設計 4）。

## 他チケットへ提供するインターフェース

- 「Windows の Tsuzuri のプログラムの引数 = UCRT の `argv[1..]`」という契約。[W11](W11-windows-os-api.md) の `Process.run` の引用（D9: MSVC の CRT と `CommandLineToArgvW` が元に戻せる形）は、子が Tsuzuri のプログラムでも引数を往復できる。W11 の `Env.args` は同じ関数を使う（D4）。
- [W09](_completed/W09-windows-gui-subsystem.md) の GUI サブシステムの実行ファイルも、同じ `def main :: Array<string> -> i32` の経路でコマンドラインを受け取るので、同じ規則になる（W09 は `wWinMain` を生成しない）。

## 実装手順

### 手順 1: 基準

- 変更: なし（`target/scratch/w33/` の下だけ）。
- 内容: `node scripts/repo-kb.mjs brief W33`。今の開発版で、`tests/fixtures`・`examples` の `Main.tz` と「早見」の `script.tz` を `--emit llvm`（`-O0`・`-O3`、native・`--target wasm32`・`--target x86_64-pc-windows-msvc`）にして保存する。「現状」の再現 1 を記録する。
- 確認: 保存した数を控える。

### 手順 2: 表と往復のテスト（D1・D3・D4 の承認後）

- 変更: `tests/arguments_runtime.c`、`tests/arguments.rs`、`tests/script.mjs`。
- 内容: テスト計画 1・2・4。期待値を先に書く。
- 確認: `cargo test --locked --test arguments`（`running 2 tests` を確かめる）が、今の分割では表の変わる行（テスト計画 1 の 4 行と足した行）で失敗し、Windows では往復のテストも失敗することを確かめる。

### 手順 3: 分割

- 変更: `src/runtime/arguments.c`（W11 が先なら `src/runtime/arguments-split.h`）。
- 内容: 設計 1。
- 確認: POSIX と Windows の両方で `cargo test --locked --test arguments` が通る。手順 1 の IR が `cmp` で一致する（S2）。受け入れ条件の前処理後の本文の比較。

### 手順 4: Windows の差分テスト

- 変更: `tests/arguments_reference.c`（新規）、`tests/windows.mjs`。
- 内容: テスト計画 3。
- 確認: `cargo build` の後 `node tests/windows.mjs target/debug/tsuzuri.exe` が MSVC で通る。同梱の `tsuzuri-clang` を `TSUZURI_CLANG` にした手元の実行でも同じ段が通る（参照のプログラムは `-municode` を付けてコンパイルする。記録する）。

### 手順 5: 文書

- 変更: 「ドキュメント」のページ。
- 確認: `node scripts/check-docs.mjs _tsuzuri/language-reference/built-in-types-and-modules/env.md _tsuzuri/language-reference/compiler/usage.md`、`node scripts/check-cli-docs.mjs`。T2。PR に `ci:full` を付け、`windows.yml`（x64・Arm64）が通る。

## テスト計画

1. **表**（`tests/arguments_runtime.c`、すべてのホスト）: 期待値は「仕様」の規則から手で書く（GUIDE-LITE §2 規則 10。コンパイラやテストの出力から写さない）。起票時に、下のすべての行を UCRT の `argv` と照らした（「現状」の再現 3）。
   - 今の行のうち期待値が変わるもの: `u"  app  "` は `u"app"` の 1 つ（先頭の空白で、プログラム名が空）、`u"app \"a\\\" b"` は `u"a\" b"` の 1 つ、`u"app C:\\dir\\ \"x\\\\\""` は `u"C:\\dir\\"`・`u"x\\"`、`u"app \"\"\"\""` は `u"\""`。ほかの今の行（`""`、`app`、`"C:\Program Files\app.exe" a`、`""arg1 arg2 arg3""`、`a "" b`、`a"b c"d`、`"unterminated b`、`"" a`、日本語と絵文字）は同じ期待値のまま。
   - 足す行: 「現状」の表の全行（`a\"b`、`"a\" b"`、`"C:\dir\"`、`"C:\dir\\"`、`C:\dir\ x`、`a\\\\"b c"`、`"a\\\"b c"`、`\\"`、`"a""b"`、`"x""y"z"`、`"a"" b"`、`"""""`、改行を含む引数、先頭のタブ、`C:\dir\"x y" z`）と、`\"`（`"`）、`"`（空の 1 つ）、`"""`（`"`）、`a\\"b`（`a\b`）、`x\y \\server\share`（2 つ、バックスラッシュはそのまま）、末尾のバックスラッシュ `app a\`（`a\`）。
2. **往復**（`tests/arguments.rs` の `executables_receive_their_command_line_arguments`、すべてのホスト。Windows の CI は nextest で走らせる）: 渡す引数に `a"b`、`C:\dir with space\`、リテラルの `""`、`x\\y` を足す（Rust の `Command` が Windows で CRT の規則で引用する）。期待する標準出力と終了コードはすべての OS で同じ。今の HEAD の Windows では失敗する。
3. **UCRT との差分**（`tests/windows.mjs`、Windows だけ）: `tests/arguments_reference.c` は、`wmain` の `argv[1..]` を「早見」の `script.tz` と同じ形（`[` と、`][` でつないだ引数と、`]` と改行）で UTF-8（`WideCharToMultiByte(CP_UTF8, ...)`）で書き、引数の数を終了コードにする C のプログラム（標準出力は `_setmode(_fileno(stdout), _O_BINARY)` にして、引数の中の改行を `\r\n` に変えない）。`nativeFlags` で同じ clang でコンパイルする。Tsuzuri の側は `script.tz` と同じ本文を `-O0`・`-O3` で build する。標準出力と終了コードの両方を比べる（引数がないときと空の引数 1 つのときは、出力が同じ `[]` で終了コードが違う）。行の候補は孤立サロゲートを含まない（UTF-8 で比べるため）。生のコマンドラインは Node の `spawnSync(exe, [tail], { windowsVerbatimArguments: true, argv0 })` で渡す（コマンドラインは `argv0 + " " + tail` になる。起票時に確認）。`argv0` を `app`、`␠␠app`、`"C:\Program Files\app.exe"`、`C:\dir\"x` にした行と、テスト計画 1 のすべての `tail` で、2 つの出力が一致すること。参照は本物の UCRT なので、期待値をコンパイラの出力から写すことにならない。
4. **`tsuzuri script`**（`tests/script.mjs`）: 「Everything after the file goes to the program unchanged」の行の引数に `a"b` と `C:\dir with space\` を足す。`script.mjs` は `windows.yml` で走らないので、Windows の検証は 2 と 3 が担う（POSIX の回帰の検査として足す）。
5. **IR**: 手順 1 の IR が変わらない（S2）。

## ドキュメント

- `_tsuzuri/language-reference/built-in-types-and-modules/env.md` の「引数と main」: 「バックスラッシュに特別なエスケープはありません」を、「Windows では、C や C++ のプログラム（Visual Studio 2008 以降の CRT）と同じ規則で分けます」と規則の要約（`\"` はリテラルの `"`、`"` の前のバックスラッシュは半分、引用の中の `""` はリテラルの `"`、ほかのバックスラッシュはそのまま）に置き換える。末尾のバックスラッシュの引用（`"C:\dir\"`）の注意と書き直し方（`"C:\dir\\"` か `"C:\dir"`）を書く。
- `docs/language.md` の「アプリケーションのエントリーポイント」の 3 つ目の箇条: `"C:\dir\"` の例を、新しい規則の例（`"C:\dir\\"` は `C:\dir\`、`a\"b` は `a"b`）に置き換え、`"C:\dir\"` は `C:\dir"` になる（後ろの引数も同じ引数に入る）と書く。
- `docs/architecture.md` の「`def main` の入口」: 分割の規則の文と、`tests/arguments.rs`・`tests/arguments_runtime.c`・`tests/windows.mjs` の検証の文。
- `_tsuzuri/language-reference/compiler/usage.md` の「script」: Windows でも引数がそのまま届くこと（`tsuzuri script` が CRT の規則で引用し、プログラムが同じ規則で分ける）を 1 文足す。
- `docs/stability.md`: edition と無関係に変えた実行時の規則として、「Windows の `def main :: Array<string> -> i32` の引数の分割を、UCRT（Visual Studio 2008 以降）と同じ規則にした（W33）。`\"` と `"` の前のバックスラッシュの扱いが変わり、`"C:\dir\"` は `C:\dir"` になる」を足す（置き場は「既知の例外」の項目か、その下の小節。D4）。
- `_tsuzuri/language-reference/organizing-tsuzuri/modules.md`（「`"..."` で囲んだ空白は 1 つの引数になります」）は新しい規則でも正しいので変えない。`README.md` は分割の規則を書いていない（起票時に `grep` で確認）ので変えない。旧い `_docs/` はもう無い（起票時に確認）。

## 受け入れ条件

- [ ] 「仕様」の 1〜5 のとおりに動き、テスト計画 1〜4 が通る（3 は MSVC の CI で。同梱の MinGW でも手元で通し、記録する）。
- [ ] どのプログラムの `--emit llvm` も変わらない。POSIX と WASI の引数の結果が変わらない。
- [ ] POSIX と WASI の `arguments.c` の前処理後の本文が、空行を除いて変更の前後で同じ（同梱の `tsuzuri-clang` の `-target x86_64-linux-gnu -std=c11 -E -P src/runtime/arguments.c`、`-target aarch64-macos` の同じもの、`-target wasm32-wasi` の同じもの）。変わるのは `#if defined(_WIN32) || defined(TZ_ARGUMENTS_TEST)` の中だけ。
- [ ] `def main :: Array<string> -> i32` を使わないプログラムは影響を受けない（`arguments.c` を連結しない。driver の `arguments_runtime` は変えない）。
- [ ] 分割の C が 1 か所だけにある（W11 の `arguments-split.h` があればそこ）。
- [ ] 文書を直し、`check-docs` と `check-cli-docs` が通る。PR に `ci:full` を付け、`windows.yml` が通る。
- [ ] PR の説明に、利用者に見える変化（`"C:\dir\"` の例と、バッチファイルの `"%~dp0"`）と承認の記録を書く。

## 落とし穴

- **末尾のバックスラッシュの引用**: CRT の規則では、`\"` はリテラルの `"` なので、`"C:\dir\"` は `C:\dir"` になり、閉じない引用が後ろの引数を巻き込む。起票時に、`dp0.bat` に `"%~dp0probe.exe" "%~dp0" "%~dp0." "%~dp0\" x` と書いて実行すると、C のプログラム（UCRT）は `C:\…\c" C:\…\c\. C:\…\c\ x` の 1 つを受け取った。`"C:\" "D:\Data\" x` は `C:" D:\Data"`・`x` の 2 つ。今の Tsuzuri はどちらも区切り通りに受け取る（前者は 4 つ、後者は 3 つ）。これは C・C++・Rust のプログラムと同じ振る舞いになるという変更で、書き直し方は `"%~dp0."`・`"%~dp0\"`（末尾を `\\` にする）・末尾の `\` を書かない、のどれか。文書と PR の説明に書く。
- プログラム名は引数と違う規則で読む（バックスラッシュを解釈しない）。`"C:\Program Files\app.exe"` を引数の規則で読むと、パスの `\` は残るが、`C:\dir\"x y"` のような名前で食い違う。
- 引用の中の `""` は、UCRT（2008 以降）ではリテラルの `"` で引用が続き、`CommandLineToArgvW` では引用が閉じる。表のテストは UCRT の値を書く。Windows の引用の書き手は `""` を出さないので、往復には効かない。
- 先頭の空白: `CreateProcessW` に `lpApplicationName` と空白で始まるコマンドラインを渡す起動器では、最初の語が引数になる（C のプログラムと同じ）。今の Tsuzuri は最初の語を捨てる。
- 2 回の走査は同じ関数で数えて埋める。1 回目と 2 回目で規則が違うと、確保した長さを超えて書く。
- `#if defined(_WIN32) || defined(TZ_ARGUMENTS_TEST)` の区切りを保つ。`tests/arguments.rs` は POSIX の機械でも `tests/arguments_runtime.c` を `-Werror` でコンパイルする。
- Node の `windowsVerbatimArguments` で生のコマンドラインを渡すときは、`argv0` が先頭の語になる。`argv0` を渡さないと、空白を含む一時ディレクトリのパスがプログラム名を割り、参照と Tsuzuri の出力が実行ファイルの名前の分だけ違う。
- 事前ビルドのランタイムは C の本文で照合するので、変更の直後は外れて Clang でコンパイルする（正しさには効かない）。

## 対象外

- Windows の `Env.args` の実装（[W11](W11-windows-os-api.md) の D4。同じ関数を使う）。
- `Process.run` の引用（W11 の D9）と、`cmd.exe`・バッチファイルの引用（W11 の D8、BatBadBut）。
- `tsuzuri run` がプログラムに引数を渡すこと（今は渡さない。別の判断）。
- 2008 より前の MSVC の CRT（`msvcrt.dll`）の規則、`WinMain` の `lpCmdLine`。
- POSIX と WASI の引数。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | Windows の `main` の引数を、UCRT（Visual Studio 2008 以降）が `wmain` に渡す `argv[1..]` と同じ規則で分ける（`CommandLineToArgvW` とは引用の中の `""` と、プログラム名の中の `"` の 3 つの場合で違う） | Windows の引用の書き手（Rust の `Command`、Node、`tsuzuri script`、W11 の D9）はどれも CRT の規則で引用し、`tsuzuri script` の「そのまま渡る」もこの規則でしか成り立たない。MSVC と MinGW（UCRT）の C のプログラムが受け取る配列と同じになり、本物の CRT を参照にして試験できる。今の規則はリテラルの `"` を表せず、選んだ記録もない | 要承認（承認前は該当する段に着手しない） |
| D2 | 規則は自前の C で実装する（`CommandLineToArgvW` も CRT の `__wargv` も使わない） | `CommandLineToArgvW` は D1 の規則と違い、`shell32` の読み込みが要る。`__wargv` はナローの `main` の入口では埋まらず（MinGW と MSVC で確認）、CRT の起動の内部の関数に頼ることになる。どちらも POSIX の機械での表のテスト（`TZ_ARGUMENTS_TEST`）を失う | 既定案（実装者はこの案に従う） |
| D3 | 先頭が空白・タブのコマンドラインも UCRT に合わせる（プログラム名は空、最初の語は引数） | 契約を「UCRT の `argv[1..]`」の 1 文にでき、例外なく差分テストで確かめられる。UCRT と `CommandLineToArgvW` はこの点で一致する | 要承認（承認前は該当する段に着手しない） |
| D4 | 変更は edition に結び付けず、`docs/stability.md` に記録する | ランタイムは edition を知らず、`main` の引数と `Env.args` は同じ規則でなければならない。edition で分けると、2026 のパッケージのプログラムが壊れた往復を持ち続ける | 要承認（承認前は該当する段に着手しない） |
| D5 | 分割の実装は 1 つだけにし、W11 の `Env.args` も同じ関数を使う（W11 の D4 を保つ） | 「`main` の引数と `Env.args` は同じ個数」の約束を、実装の共有で自動的に守る | 既定案（実装者はこの案に従う） |
| D6 | 旧い規則に戻す切り替え（環境変数・フラグ）を設けない | 切り替えは D-55 の登録になり、2 つの規則を持ち続ける。旧い規則に頼るコマンドラインは、C のプログラムと同じ書き方（落とし穴）に直せる | 既定案（実装者はこの案に従う） |
