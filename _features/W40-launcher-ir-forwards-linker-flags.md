# W40: 同梱の launcher の `-x ir` の経路が `-Wl,--gc-sections` を IR のコンパイルに渡し、Windows のすべてのビルドで警告が出る

| 項目 | 内容 |
| --- | --- |
| ID | W40 |
| 優先度 | P3（修正。ビルドは成功し、出力も変わらない。ただし、同梱ツールチェーンで `-g` のない native の実行ファイルを作るたびに、`build`・`run` の標準エラーへ Clang の警告が 1〜3 行出る。利用者は自分の誤りと区別できない） |
| 規模 | S（launcher の純粋関数 1 つとその呼び出し、`tests/targets.rs` の単体テスト、`scripts/toolchain/smoke.mjs` の段、`docs/architecture.md` の 1 文） |
| 依存 | – |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。W01〜W37 の起票中に見つけ、利用者の依頼「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」に当たるので起票。HEAD `fa40eede` で確認） |
| 承認 | 不要（Tsuzuri の言語の意味、生成 IR、object、診断、フラグ、環境変数は変えない。同梱の launcher が IR のコンパイルに渡す引数から、リンカーと前処理器への引数を除くだけで、リンクの実行の引数は今と同じ。台帳は [GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30)） |
| 手本にする既存実装 | `scripts/toolchain/clang.rs` の `is_dropped`（Zig と Clang の違いに合わせて引数を選ぶ小さな関数）と `resolve`（`tests/targets.rs` が `#[path = "../scripts/toolchain/clang.rs"] mod shim` で単体テストする純粋関数）、[W37](W37-bundled-clang-ubsan-objects.md) の `ubsan_trap`（同じ形の計画中の関数） |
| 主な影響ファイル | `scripts/toolchain/clang.rs`、`tests/targets.rs`、`scripts/toolchain/smoke.mjs`、`docs/architecture.md` |

## 実装者向け早見

- 範囲: 同梱の launcher（`scripts/toolchain/clang.rs` の `main`）の `-x ir` の枝が、IR のコンパイル（同梱の LLVM Clang の `-x ir -c`）に渡す引数の選別だけ。
- 原因: 選別の `text.starts_with("-W")` が、警告の指定（`-Wno-override-module`）と一緒に、リンカーへの引数（`-Wl,--gc-sections` など）も拾う。コンパイルだけの Clang はそれを使わないので警告する。
- 直し方: 選別を `pub fn compiles_ir(text: &str) -> bool` に出し、`-Wl,` と `-Wp,` で始まる引数を除く。リンクの実行（`zig cc`、ARM64 Windows の LLVM Clang）に渡す `arguments` は変えない。
- 最初に読む: `clang.rs` の `main` の `if let Some(language) = arguments.windows(2).position(..)` の枝 → `src/driver.rs` の `dead_strip_args` とその呼び出し（`build_complete` のリンクの引数）。
- 最初に書くテスト: `tests/targets.rs` の `the_bundled_launcher_keeps_linker_arguments_out_of_the_ir_compile`。
- 触ってはいけない: `zig cc` の枝（W37 が変える）、ARM64 Windows の llvm-mingw の枝、`resolve`・`NATIVE_TARGETS`・`is_dropped`、`src/` のすべて。
- 完了の見分け方: 新しい同梱ツールチェーンで `tsuzuri build app -o app.exe`（`-g` なし）の標準エラーが空になる（今は下の 1 行）。

```sh
T=$HOME/.vscode/extensions/tmidorikawa.tsuzuri-0.1.6-win32-x64/toolchain
TSUZURI_CLANG=$T/bin/tsuzuri-clang.exe target/debug/tsuzuri.exe build app -o app.exe
# clang: warning: -Wl,--gc-sections: 'linker' input unused [-Wunused-command-line-argument]
```

## 目的

同梱の launcher は、`-x ir` の実行を 2 つに分ける。IR は同梱の LLVM Clang で object にし（Zig は IR を読まない）、その object とほかの入力を `zig cc` でリンクする。コンパイラは 1 回の Clang の実行でコンパイルとリンクをする前提で、リンカーへの引数も同じ実行に渡す。launcher はそれを IR のコンパイルにも渡してしまい、Clang が「使われない」と警告する。

IR のコンパイルにはコンパイルの引数だけを渡し、リンカーへの引数はリンクの実行にだけ渡す。警告を消し、出力は 1 byte も変えない。

## 着手条件と停止条件

### 着手条件

- `node scripts/repo-kb.mjs brief W40`。`scripts/toolchain/` を変えるので、AGENTS.md に従って `.github/instructions/ci.instructions.md` を読む。
- 「現状」の再現を、HEAD の同梱ツールチェーン（VS Code の拡張の 0.1.6、または `node scripts/toolchain/bundle.mjs` で作ったもの）で繰り返す。結果が違えば、このチケットを直してから始める。
- [W37](W37-bundled-clang-ubsan-objects.md) が `main` に入っているかを `grep -n "ubsan_trap" scripts/toolchain/clang.rs` で確かめる（「W37 との関係」）。

### 停止条件

- **S1** 新しい launcher で作った object が 1 byte でも変わる（実装手順 3 の配布物のランタイム object の比較）。除いた引数が出力に効いていたことになるので、止めて報告する。
- **S2** リンクの実行が今の引数を受け取らなくなる（smoke の実行ファイルが大きくなる、`-Wl,-u,tz_<名前>` の export が消えるなど）。
- **S3** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

Git Bash で、開発版のコンパイラ（`target/debug/tsuzuri.exe`）と VS Code の拡張に同梱のツールチェーン（0.1.6、LLVM Clang 21.1.8）を使った。作業は `target/scratch/w-w38w40/` で行い、確認後に消した。

### 警告が出るビルド

| ビルド（`TSUZURI_CLANG=$T/bin/tsuzuri-clang.exe`） | 標準エラー |
| --- | --- |
| `build app -o app.exe`（既定の `-O3`）、`-O0`、`run app --no-cache`、`--target x86_64-pc-windows-msvc` | `clang: warning: -Wl,--gc-sections: 'linker' input unused [-Wunused-command-line-argument]` の 1 行 |
| `export def` を持つ `lib` を `build lib --target x86_64-unknown-linux-musl --strip` | `-Wl,-x`・`-Wl,--gc-sections`・`-Wl,-u,tz_answer` の 3 行 |
| `-g`、`--emit object` | なし（リンクの GC を使わない） |

- どのビルドも成功し、実行ファイルは期待どおりに動いた（`app.exe` は終了コード 42）。
- Linux の target でも出るので、原因はホストの OS によらない。launcher のコードはすべてのホストで同じである。ただし、Linux・macOS のホストの配布物では確かめていない（未確認）。題は「Windows のすべてのビルド」だが、実際の範囲は「どのホストでも、同梱ツールチェーンの `-g` のない native の実行ファイルのビルド」である。

### コード

- `scripts/toolchain/clang.rs` の `main`: `-x ir` の 2 つの引数を取り除き、`.ll` の入力を探す。同梱の LLVM Clang に `-target <LLVM の target> -x ir -c` を渡し、続けて `-O`・`-W`・`-m` で始まる引数と `-g`・`-fPIC`・`-flto*`・`-ffunction-sections`・`-fdata-sections` を渡す（175〜189 行目付近の `for argument in &arguments`）。`-c` があればそこで返る。なければ object を一時ディレクトリの `module.o` に書き、`arguments` の `.ll` をそれに置き換えて、すべての引数を `zig cc`（ARM64 Windows では LLVM Clang）のリンクに渡す。
- `src/driver.rs` の `build_complete`: `gc_sections` は `Emit::Executable && Target::Native && !debug_info`（3951 行目付近）。リンクの Clang の実行は `-x ir -Wno-override-module <最適化>` で始まり、`gc_sections` なら `dead_strip_args(platform)` を足す（5214 行目付近）。`dead_strip_args` は Mach-O で `-Wl,-dead_strip`、同梱の launcher でない MSVC で `-Wl,/OPT:REF`、ほかは `-Wl,--gc-sections`。続けて、Windows 以外では export する関数ごとに `-Wl,-u,tz_<名前>`（Mach-O は `_tz_`）を足す。`-Wl,-x` はその前に、Windows 以外の target の実行ファイルで `--strip` なら足す（`gc_sections` の条件の外にあるので `-g` でも足す。そのため Windows 以外の target の `--strip -g` のビルドも警告を出すはずである。未確認）。
- システムの Clang（`TSUZURI_CLANG` が LLVM の `clang`）は 1 回の実行でコンパイルとリンクをするので、`-Wl,` はリンクで使われ、この警告は出ない。分けるのは launcher だけである。

### 同梱の Clang の `-x ir -c` に引数を足した結果（`m.ll` は `define i32 @f()` だけの IR）

| 足した引数 | 結果 |
| --- | --- |
| `-Wl,--gc-sections`、`-Wl,-u,tz_x`、`-Xlinker --gc-sections` | `'linker' input unused` の警告 |
| `-Wp,-DX`、`-mwindows` | `argument unused during compilation` の警告 |
| `-Wa,--noexecstack`、`-municode`、`-march=x86-64-v2` | 警告なし |
| `-Wno-override-module` | 警告なし。外すと `warning: overriding the module target triple with x86_64-w64-windows-gnu [-Woverride-module]` が出る（警告の指定は渡し続ける必要がある） |

- 出力は変わらない: `lib` の IR（`build lib --emit llvm -O3`）を `-target x86_64-w64-windows-gnu -x ir -c -Wno-override-module -O3 -ffunction-sections -fdata-sections` でコンパイルした object は、`-Wl,--gc-sections -Wl,-u,tz_answer` を足しても `cmp` で一致した。`-target x86_64-unknown-linux-musl` に `-Wl,--gc-sections -Wl,-x -Wl,-u,tz_answer` を足した ELF も一致した。
- 実行ファイルどうしは比べられない: 同じ `build app2 --no-cache` を 2 回行うと、20 バイトが違う（COFF ヘッダーの TimeDateStamp（0x80）とデバッグディレクトリの時刻）。

## 仕様

1. 同梱の launcher は、`-x ir` の実行の IR のコンパイルに、今の選別から `-Wl,` か `-Wp,` で始まる引数を除いたものを渡す。警告の指定（`-W<名前>`・`-Wno-<名前>`）と `-Wa,` は今どおり渡す。
2. リンクの実行に渡す引数（`.ll` を `module.o` に置き換えた `arguments`）は今と同じ。`-Wl,--gc-sections`・`-Wl,-dead_strip`・`-Wl,-u,…`・`-Wl,-x` は今どおりリンカーに届く。
3. 結果: 同梱ツールチェーンの `-g` のない native の実行ファイルのビルドで、`-Wunused-command-line-argument` の警告が出ない。IR のコンパイルの object、実行ファイル（時刻を除く）、配布物のランタイム object は 1 byte も変わらない。
4. 新しいフラグ・環境変数・manifest のキーは足さない（D-55 の登録はない）。

## 設計

`scripts/toolchain/clang.rs`:

```rust
/// W40: whether an argument of a `-x ir` run also goes to the compile of the IR, which reads compile
/// options only. Linker (`-Wl,`) and preprocessor (`-Wp,`) arguments go to the link run alone: a
/// compile warns that they are unused, and the link run still receives every argument.
pub fn compiles_ir(text: &str) -> bool {
    if text.starts_with("-Wl,") || text.starts_with("-Wp,") {
        return false;
    }
    text.starts_with("-O")
        || text.starts_with("-W")
        || text.starts_with("-m")
        || text == "-g"
        || text == "-fPIC"
        // `-flto=thin` makes this compile write bitcode with the ThinLTO summary (PR08).
        || text.starts_with("-flto")
        || text == "-ffunction-sections"
        || text == "-fdata-sections"
}
```

- 呼び出しは `-x ir` の枝の 1 か所: `if compiles_ir(&argument.to_string_lossy()) { compilation.arg(argument); }`。`to_string_lossy` は今と同じ（UTF-8 でないパスは選ばれない）。
- `arguments` は変えないので、`-c` のない実行のリンク（`zig cc` と ARM64 Windows の LLVM Clang）は今と同じ引数を受け取る。
- `tests/targets.rs` は `mod shim` でこのファイルを読むので、`pub fn` はそのまま単体テストできる。`scripts/toolchain/bundle.mjs` は `rustc --edition=2024 -O` でこのファイルだけをコンパイルするので、crate を足さない。

### 採らない案

- **IR のコンパイルに `-Qunused-arguments`（または `-Wno-unused-command-line-argument`）を足す**: 警告は消える。しかし、本当に効いていない引数（将来の誤った指定）の警告も隠す。除く方が正確である。
- **driver が同梱の launcher に `dead_strip_args` を渡さない**: リンクの GC が失われる（`usage.md` の「未使用の関数とデータをリンク時に除去」に反する）。driver の側でコンパイルとリンクを分けるのは、プロセスが増え、変更も大きい。launcher の 2 段の分割の都合は launcher の中で吸収する（`is_dropped` と同じ）。
- **`-Wl,--gc-sections` だけを除く**: `-Wl,-u,…`・`-Wl,-x`・`-Wl,-dead_strip` と、W09 の `-Wl,--subsystem,windows` の警告が残る。

## 前提とする他チケットのインターフェース

- なし。

### W37 との関係

- この不具合は、[W37](W37-bundled-clang-ubsan-objects.md) の「対象外」の行（`-x ir` の経路が `-W` で始まる引数をすべて LLVM Clang に渡す）から起票した。その行は、このチケットを指す。
- 同じ `scripts/toolchain/clang.rs` と `tests/targets.rs` を触る。W37 は `zig cc` の枝と新しい `ubsan_trap`・`UBSAN_TRAP` を、このチケットは `-x ir` の枝の選別と新しい `compiles_ir` を変える。行は重ならないが、どちらも `is_dropped` の近くに `pub fn` を足し、`tests/targets.rs` の末尾にテストを足すので、後から入る方が rebase で確かめる。
- 意味は独立している。W37 は、IR のコンパイルの後に残る `arguments` で判定する（W37 の仕様 2）。このチケットは `arguments` を変えない。W37 は「`-x ir` の経路を触らない」としているので、W37 の前後でこのチケットの選別は同じである。
- どちらも launcher の SHA-256 を変える。そのため配布物のランタイム object とキャッシュは、どちらの PR の後でも 1 度作り直される（`src/runtime_cache.rs` の `prebuilt`、`src/cache.rs` の `tool-binary`）。両方を続けて入れれば作り直しは 1 回で済むが、順はどちらでもよい。

## 他チケットへ提供するインターフェース

- `scripts/toolchain/clang.rs` の `pub fn compiles_ir(text: &str) -> bool`。
- [W09](_completed/W09-windows-gui-subsystem.md) の MinGW の `-Wl,--subsystem,windows`（W09 の D4）は、このチケットの後は同梱の launcher の IR のコンパイルに渡らず、警告を足さない。W09 はこのチケットに依存しない（警告が増えるだけで、ビルドは成功する）。

## 実装手順

1. **変更**: なし（ブランチ `fix/w40-launcher-ir-linker-arguments`）。
   **内容**: 着手条件を確かめる。HEAD の launcher を作る（`rustc --edition=2024 -O scripts/toolchain/clang.rs -o target/scratch/w40/old/bin/tsuzuri-clang`）。`TSUZURI_ZIG`・`TSUZURI_LLVM_CLANG` を同梱の Zig と Clang に向け、`node scripts/toolchain/runtime.mjs target/scratch/w40/dist target/scratch/w40/old/bin/tsuzuri-clang` を実行する。その `share/runtime/manifest.json` を `target/scratch/w40/old-manifest.json` に写す（W37 の実装手順 1 と同じ）。
   **確認**: 「現状」の警告が再現する。
2. **変更**: `tests/targets.rs` に `the_bundled_launcher_keeps_linker_arguments_out_of_the_ir_compile`（テスト計画）を先に書く。
   **確認**: `cargo test --locked --test targets the_bundled_launcher_keeps` がコンパイルで失敗する（`shim::compiles_ir` が無い）。
3. **変更**: `scripts/toolchain/clang.rs`（設計）。
   **確認**: 手順 2 のテストが `running 1 test` で通る。T0。新しい launcher を `target/scratch/w40/new/bin/tsuzuri-clang` に作り、手順 1 と同じ `target/scratch/w40/dist` に `runtime.mjs` をもう一度実行する。manifest の `objects` の sha256 がすべて同じで、変わるのは `tools` の launcher の SHA-256 だけであることを確かめる（S1）。新しい launcher を `TSUZURI_CLANG` にして、「現状」の表のビルドの標準エラーに警告が無く、実行ファイルが動くことを確かめる。
4. **変更**: `scripts/toolchain/smoke.mjs`（テスト計画の段）。
   **確認**: `node scripts/slot.mjs -- node scripts/toolchain/bundle.mjs target/toolchain` の後、`node scripts/toolchain/smoke.mjs target/toolchain` が通る。HEAD の launcher に置き換えた写しでは、足した検査が失敗することを 1 度確かめる。
5. **変更**: 文書（下）。
   **確認**: T1（`cargo nextest run --locked`）、T2。PR に `ci:full` を付ける（`scripts/toolchain/**` を変えるので、`targets.yml` と、smoke を回す `vscode.yml` も動く）。`ci.yml`・`targets.yml`・`windows.yml`・`vscode.yml` の 5 環境が通ることを確かめ、`target/scratch/w40/` を消す。

## テスト計画

### 単体（`tests/targets.rs`）

`the_bundled_launcher_keeps_linker_arguments_out_of_the_ir_compile`: 引数と期待値の表。期待値は「現状」の測定から手で書く。

| 引数 | 期待 |
| --- | --- |
| `-O0`、`-O3`、`-Os`、`-Wno-override-module`、`-Wa,--noexecstack` | true |
| `-march=x86-64-v3`、`-mf16c`、`-mbulk-memory`、`-g`、`-fPIC`、`-flto=thin`、`-ffunction-sections`、`-fdata-sections` | true |
| `-Wl,--gc-sections`、`-Wl,-dead_strip`、`-Wl,-u,tz_answer`、`-Wl,-x`、`-Wl,--subsystem,windows`、`-Wp,-DX` | false |
| `-c`、`-o`、`-x`、`none`、`-lws2_32`、`-pthread`、`-std=c11`、`-Xlinker`、`/PDB:a.pdb`、`a.ll` | false |

### smoke（`scripts/toolchain/smoke.mjs`。`vscode.yml` の 5 環境）

- **S-a**（全環境）: 今の smoke の `tsuzuri new` で作ったプロジェクトの 2 つのビルド（`build <created> --no-cache` と `build <created> -Oz --strip --no-cache`。どちらも `-g` がなく、リンクの GC の経路を通る。後者は Windows 以外で `-Wl,-x` も通る）の結果を受け取る。その `result.stderr` が `-Wunused-command-line-argument` を含まないことを `assert.doesNotMatch` で確かめる（smoke の `run` は結果を返す）。今のこの 2 つのビルドも「現状」と同じ経路で警告を出すはずだが（smoke の中では未確認）、smoke は標準エラーを見ていない。
- `-Wl,-u,…`（export のある Linux・macOS のビルド）は、Clang の扱いが同じ `-Wl,` なので、規則を単体テストで固定するだけにする。

## ドキュメント

- `docs/architecture.md` の同梱ツールチェーンの段落（447 行目付近の `scripts/toolchain/clang.rs` の文）に 1 文足す。内容は次の 2 つ。`-x ir` の経路は、IR のコンパイルにコンパイルの引数だけを渡し、`-Wl,`・`-Wp,` で始まる引数はリンクの実行にだけ渡す。その選別は `compiles_ir` で、`tests/targets.rs` と smoke が確かめる。
- 利用者向けの文書は変えない。この警告はどのページにも書かれていない（`grep -rn "input unused" _tsuzuri` が 0 件）。リンクの GC の説明（`compiler/usage.md` の 1092 行目付近）は今も正しい。README と `docs/language.md` も変えない。

## 受け入れ条件

- [ ] 同梱ツールチェーンの `-g` のない native の実行ファイルのビルド（Windows の target と、Windows のホストからの `x86_64-unknown-linux-musl` の `--strip` と export）で、`-Wunused-command-line-argument` の警告が出ない。
- [ ] 配布物のランタイム object（同じ場所で作った `share/runtime`）が 1 byte も変わらず、manifest で変わるのは `tools` だけ。
- [ ] リンクの実行の引数が変わらない（単体テストと、smoke の実行ファイルが動くこと）。`src/` を変えていない。
- [ ] `the_bundled_launcher_keeps_linker_arguments_out_of_the_ir_compile` と smoke の S-a が通る。
- [ ] T2 が通る。PR に `ci:full` を付け、`vscode.yml` の 5 環境と `windows.yml`・`targets.yml` が通る。

## 落とし穴

- **`-W` をまとめて除かない**: `-Wno-override-module` を外すと、すべてのビルドで「overriding the module target triple」の警告が出る（「現状」の表）。除くのは、ほかのツールへ渡す `-Wl,`・`-Wp,` だけにする。
- **`-Wa,` は残す**: 統合アセンブラーが IR のコンパイルで読む。警告も出ない。
- **リンクの引数を変えない**: 選別は IR のコンパイルの `compilation` にだけ効かせる。`arguments` から取り除くと、`-Wl,--gc-sections` がリンカーに届かず、実行ファイルが大きくなる（S2）。
- **実行ファイルのバイト比較**: PE の時刻が違うので使えない。object と配布物の manifest で比べる（「現状」）。

## 対象外

- `-Xlinker` の対の扱い。`-Xlinker` そのものは今も選ばれないが、次の値が `-O`・`-W`・`-m` で始まると IR のコンパイルに渡る。driver が `-Xlinker` を使うのは `pdb_link_args`（`/PDB:` など、`/` で始まる値）だけで、同梱の launcher には届かない。
- `-mwindows`・`-mconsole` などの MinGW のリンクの指定（`-m` で始まるので IR のコンパイルに渡り、警告する）。driver は渡さず、W09 も `-Wl,--subsystem,windows` を使う（W09 の D4）。
- 利用者が `tsuzuri-clang -x ir` を直接使うときの、ほかの引数の扱い。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 直す場所は launcher の `-x ir` の枝の選別（純粋関数 `compiles_ir`）。driver は変えない | コンパイルとリンクを 2 段に分けるのは launcher だけで、システムの Clang では問題が起きない。Zig と LLVM Clang の違いを吸収するのは launcher の役目である（`is_dropped`・W37） | 既定案（実装者はこの案に従う） |
| D2 | IR のコンパイルから除くのは `-Wl,` と `-Wp,` で始まる引数だけ。警告の指定と `-Wa,` は渡し続ける | 測定で警告が出るのは、リンカーと前処理器への引数だった。`-Wno-override-module` は要る。`-Wa,` は IR のコンパイルで効く | 既定案（実装者はこの案に従う） |
| D3 | リンクの実行に渡す引数は変えない | リンクの GC・export の `-u`・`--strip` は、リンカーに届いて初めて効く | 既定案（実装者はこの案に従う） |
| D4 | 警告を `-Qunused-arguments` で隠さない。新しいフラグ・環境変数も足さない | 本当に効いていない引数の警告まで隠さないため。D-55 の登録を増やさない | 既定案（実装者はこの案に従う） |
| D5 | smoke に、`-g` のないビルドの標準エラーに `-Wunused-command-line-argument` が無いことの検査を足す | launcher の規則は単体テストが固定し、Clang の振る舞いは 5 環境の smoke が見る（W37 と同じ分担） | 既定案（実装者はこの案に従う） |
