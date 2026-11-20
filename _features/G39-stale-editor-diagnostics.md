# G39: VS Code で直した警告・エラーが消えない（拡張の `tsuzuri-build` とタスクの problem matcher が出す写しが残る。言語サーバーは前に送った URI を覚えていない）

| 項目 | 内容 |
| --- | --- |
| ID | G39 |
| 優先度 | P1（拡張を使う人のほぼ全員が、ふつうの編集で踏む。エラーのある状態で一度保存すると、直して保存しても問題パネルと波線に `Tsuzuri` の写しが残り、言語サーバーを再起動しても消えない） |
| 規模 | L（Phase 1 は拡張の 3 か所と統合テスト、Phase 2 は言語サーバーの publish の記録、Phase 3 は言語サーバーの要求の待ち行列と取り消し。どれも S〜M） |
| 依存 | G07, G12, G14, G16（すべて done） |
| 後続 | – |
| 状態 | doing（Phase 1 done。Phase 2・3 は実装済みで、段 13 の計測も済んだ（2026-11-20）。`ci:full` の `vscode.yml`・`windows.yml` が残る） |
| 起票 | 2026-11-19（利用者の報告「VS Code と Tsuzuri の拡張で開発すると、警告やエラーの箇所を直しても表示され続ける」を受けて起票。HEAD `af644794`（拡張 0.1.7）で、VS Code 1.103.2 に拡張と同梱のツールチェーンを入れて再現した。原因 1・2 は、利用者の環境（VS Code Insiders 1.148.0 と Marketplace の拡張 0.1.6）でも同じように再現した） |
| 承認 | 不要（言語と CLI の意味は変えない。拡張と言語サーバーが出す診断がいつ消えるかを直す。拡張のタスクが問題パネルへ診断を出さなくなること（D3）と、言語サーバーがプロジェクトを読み込めない間ほかのファイルの診断を消すこと（D6）と、根の外の依存のファイルに解析の診断を送らないこと（D9）は、利用者から見える変更なので `vsc/README.md` と言語リファレンスに書く） |
| 改善する劣位 | 不具合: (1) エラーのある状態で保存すると、問題パネルに source `Tsuzuri` の写しが増え、直して保存しても消えない。消えるのはテストを実行するか Debug したときだけ。(2) ▷ Run・Build・Check の後は、直した警告・エラーが次にタスクを流すまで残る。(3) テストの実行と Debug の後は、直した警告・エラーが次の実行まで残る。(4) ファイルの削除・名前の変更・移動、`Tsuzuri.toml` の誤り、プロジェクトの読み込みの失敗、シンボリックリンクを通したフォルダーでは、言語サーバー自身の診断（source `tsuzuri`）も残る。(5) 電球のコードアクションの要求が修正案ごとにプロジェクトを解析し直し、取り消されても最後まで計算するので、直した結果が 0.6〜3 秒、大きいプロジェクトでは 20 秒近く遅れて出る |
| 手本にする既存実装 | 拡張: `vsc/src/workflow.ts` の `publishDiagnostics`（根ごとに前の URI を消してから入れ直す）、`vsc/src/test/extension.test.ts` の「diagnostics cleared」の検査（未保存の編集だけを見ている）。言語サーバー: `src/lsp.rs` の `Session::publish`・`Session::refresh`、`tests/lsp.rs` の `scripted`・`session`（50〜130 行）、`tests/lsp_sessions.mjs` の `workspace/didChangeWatchedFiles` の場面（122〜140 行） |
| 主な影響ファイル | `vsc/src/testing.ts`, `vsc/src/workflow.ts`, `vsc/src/extension.ts`, `vsc/src/core.ts`, `vsc/src/test/extension.test.ts`, `vsc/src/test/core.test.ts`, `vsc/README.md`, `vsc/CHANGELOG.md`, `src/lsp.rs`, `src/fix.rs`, `tests/lsp.rs`, `tests/lsp_sessions.mjs`, `_tsuzuri/language-reference/compiler/diagnostics.md`, `_tsuzuri/language-reference/compiler/usage.md`, `docs/architecture.md`, `_features/README.md` |

## 目的

エディターに出る警告とエラーを、今のソースの状態と食い違わないようにする。直した箇所の診断は、直した直後に消える。
今は、同じ診断が 2 つか 3 つの出どころから別々に出ていて、言語サーバー以外の出どころは編集を見ていない。
利用者には、直したのに消えない `Tsuzuri` の写しと、生きている `tsuzuri` の診断の区別がつかない（範囲・コード・メッセージが同じで、source の大文字・小文字だけが違う）。

1. 構文・型・所有権などの前段の診断は、言語サーバーだけが出す（Phase 1）。
2. 拡張が自分で出す診断（テストの実行・Debug のビルドの結果）は、プロジェクトが変わったら消す（Phase 1）。
3. 言語サーバーは、前に診断を送った URI を覚え、今は送るものがない URI に空の一覧を送る（Phase 2）。
4. 言語サーバーは、古くなった要求や取り消された要求に時間を使わず、最新の本文の診断を先に出す（Phase 3）。

## 着手条件と停止条件

### 着手条件

- G07・G12・G14・G16 が `_features/README.md` で done であること（HEAD で done）。
- AGENTS.md の T0 と T1 が `main` の最新の `ci.yml` と同じ結果になること。
- 下の「現状」の再現 1（保存したエラーの写しが残る）を、手元の VS Code か `vsc/scripts/test-extension.mjs` と同じ方法で確かめていること。
  `vsc/` の統合テストは、`ELECTRON_RUN_AS_NODE` と `VSCODE_*` の環境変数を外して動かす（VS Code の中の端末から動かすと、テスト用の VS Code が Node として起動して失敗する）。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- 言語サーバーが報告しない診断（`E1036` のケーパビリティの検査、リンク・ツールチェーンの `E2xxx`、実行時のトラップ）が、Phase 1 の後に問題パネルにも端末にも出なくなる。
- `tests/lsp.rs`・`tests/lsp_sessions.mjs`・`vsc/src/test/extension.test.ts` の既存の期待値を、このチケットの決定事項にない理由で変えないと通らない。
- D8（クライアントの表記の URI）で、Windows のドライブ文字・UNC・`\\?\` のパスの往復が `tests/lsp.rs` の `windows_file_uris_round_trip_drive_letters_and_canonical_paths` の期待と食い違う。
- D9（根の外の依存の診断を送らない）で、根の中の依存（`path = "lib"` のような入れ子のパッケージ）の診断まで消える。
- Phase 3 で、クイックフィックスの検証（適用後にもう一度検査して確かめた修正案だけを出す）を外さないと、電球の要求による遅れが縮まない（D12）。

## 現状（2026-11-19、`main` の `af644794` で確認）

### 調べた範囲

- VS Code 1.103.2（`vsc/.vscode-test` にある統合テスト用の版）に、開発中の拡張と同梱のツールチェーンを入れて動かした。
  `vsc/dist/extension.js` は `vsc/src` から作り直したものとバイト単位で同じで、`vsc/toolchain/bin/tsuzuri` の SHA-256 は `target/release/tsuzuri` と同じ。
  起動の仕方は `vsc/scripts/test-extension.mjs` と同じ `@vscode/test-electron` で、各手順の後に `vscode.languages.getDiagnostics()` を記録した。
  問題の位置を巡る `editor.action.marker.next` で、UI 側のマーカーの一覧とも照らした。
- 言語サーバーは、vscode-languageclient 9.0.1 と同じ `initialize`（`rootUri`・`workspaceFolders`）を送る JSON-RPC の driver で、`target/debug/tsuzuri lsp` と `target/release/tsuzuri lsp` を動かした。
- CLI の出力（`check`・`build`・`run`・`test --list --json`・`test --json`）を、拡張が渡す引数（`vsc/src/core.ts` の `commandArguments`・`testDebugArguments`）で取った。
- 発見ごとに、別の担当が自分の harness で再現し直し、反証を試みた。下に「確認済み」と書いたものは両方で再現している。
- 利用者の環境でも確かめた。VS Code Insiders 1.148.0-insider に、インストール済みの Marketplace の拡張 0.1.6 を読み取り専用で読み込み、別の user-data で動かすと、原因 1・2 が同じ行で再現した（`target/scratch/g39/critic/save.out`・`task.out`）。
  0.1.6 と 0.1.7 の `vsc/src/extension.ts`・`testing.ts`・`workflow.ts` の違いは文字列 1 つで、`src/lsp.rs` の違いはシグネチャヘルプとキーワードの一覧だけです。利用者の設定は既定のまま（自動保存なし、電球は有効）。
- harness と記録は `target/scratch/g39/` にある（VS Code の harness は `vscode-e2e/runner.mjs`・`verify-vscode-e2e/runner.mjs`、言語サーバーの driver は `lsp-lifecycle/driver.mjs`・`verify-lsp-lifecycle/lsp.mjs`）。リポジトリには置かない。下の再現手順だけで作り直せる。

### 診断の出どころは 3 つある

| 出どころ | 問題パネルの source | 入れる処理 | 消える時 |
| --- | --- | --- | --- |
| 言語サーバー（`tsuzuri lsp`） | `tsuzuri` | 変更から 200 ms 後の `Session::refresh`（`src/lsp.rs:366-427`）。読み込んだプロジェクトの利用者のソースごとに送る（`src/lsp.rs:396-408`） | 次の refresh がその URI へ送るとき。送る先は今のプロジェクトのソースだけ |
| 拡張の診断の集まり `tsuzuri-build`（`vsc/src/workflow.ts:34`） | `Tsuzuri` | `--json` の stderr。テストの発見が失敗したとき（`vsc/src/testing.ts:46-48`）、テストの実行（`:200`）、テストのデバッグ（`:265`）、Debug Project のビルド（`vsc/src/workflow.ts:177`） | 同じ根で `publishDiagnostics` がもう一度呼ばれたときだけ（`vsc/src/workflow.ts:76`） |
| タスクの problem matcher `$tsuzuri`（`vsc/package.json:80-83`、owner `tsuzuri-task`） | `Tsuzuri`（行と桁だけなので幅 0 の範囲） | 拡張のタスク（`vsc/src/workflow.ts:117-119`）。▷ Run と Build のボタン（エディターの右上）、Check と Wasm のコマンド、Tasks: Run Task | 次に `$tsuzuri` のタスクが終わったとき。VS Code の仕様で、拡張からは消せない |

言語サーバーは、ふつうの編集では自分の診断を正しく消している（下の「原因ではなかったもの」）。
利用者が見ている「消えない診断」のほとんどは source `Tsuzuri` の写しで、出どころは 2 段目か 3 段目です。

### 原因 1（主因・エラー）: 保存のたびに動くテストの発見が、エラーの写しを `tsuzuri-build` に入れ、成功しても消さない

確認済み。再現 1（`tsuzuri new` で作ったプロジェクト。`Main.tz` にテストが 1 件ある）:

1. `Main.tz` の `main` の `0` を `missing` に変えて保存する（`E1002`）。問題パネルに `E1002` が 2 件並ぶ（source `tsuzuri` と `Tsuzuri`）。
2. `missing` を `0` に戻して保存する。`tsuzuri` の 1 件は約 0.6 秒で消える。`Tsuzuri` の 1 件は同じ位置に残り続ける。

次の記録は、別の harness のプロジェクト（`Main.tz` と `Helper.tz`、テスト 1 件）で、`calculate` に `let unused = 7` を足し、`let result = value + Helper.offset()` の末尾に `+ true` を足して保存し、元に戻して保存したものです。

```text
SNAPSHOT S.1 error saved +3s: Main.tz [Tsuzuri] E1003 Error @4:43-4:47 | Main.tz [tsuzuri] E1003 Error @4:43-4:47
ok after 622 ms: LSP (source tsuzuri) diagnostics cleared
SNAPSHOT S.2 fixed+saved +30s: Main.tz [Tsuzuri] E1003 Error @4:43-4:47
SNAPSHOT S.3 after restartLanguageServer +5s: Main.tz [Tsuzuri] E1003 Error @4:43-4:47
SNAPSHOT S.4 another clean save +5s: Main.tz [Tsuzuri] E1003 Error @4:43-4:47
```

（`target/scratch/g39/vscode-e2e/save.out`。同じ流れを別の harness で: `verify-vscode-e2e/save.out` の S2〜S6。`tsuzuri new` と同じ形のプロジェクトで上の再現 1 の手順を踏んだもの: `verify-build-collection/vscode/result-main.json` の M1〜M3c。利用者の環境: `critic/save.out` の S2〜S6）

仕組み:

- `vsc/src/testing.ts:112-113` の watcher は、ワークスペースのすべての `**/*.{tz,tt,tc,toml}` の作成・変更・削除を見る。
  `changed`（`:98-111`）が 300 ms 後に `discover(root)` を呼び、`discover` は `tsuzuri test <root> --list --json` を動かす（`:46`）。
- 終了コードが 0 でないときだけ、`publishDiagnostics(root, result.stderr)` を呼ぶ（`:47-48`）。成功の経路（`:51-73`）は呼ばない。
- `tsuzuri-build` の項目を消すのは `publishDiagnostics` の先頭（`vsc/src/workflow.ts:76`）だけなので、直した後の成功した発見では消えない。
  「Restart Language Server」（`vsc/src/extension.ts:80-84`）は言語クライアントだけを止めて起こすので、この集まりに触れない。

起きる条件:

- プロジェクトにエラーがある状態での保存すべて。Ctrl+S、自動保存、Run・Build・Check・Test が先に行う `saveAll`。
  `files.autoSave` が `afterDelay` なら、壊れた行で手を止めるたびに写しが入り、直して自動保存しても消えない（`verify-vscode-e2e/autosave.out` の K1・K2、`verify-build-collection` の M11・M12）。
- 保存でなくても、発見が動くときはすべて。watcher のイベント（Explorer での削除・名前の変更、`git checkout`、外のツールやエージェントの書き込み）と、ウィンドウを開いた直後に Testing の項目を解決するときの発見（`vsc/src/testing.ts:129-133` の `resolveHandler`）。
  発見はエディターのバッファではなくディスクを読むので、写しがエディターの本文と違う状態を表すことがある。
  Explorer で `Helper.tz` を消すと、未保存の編集を持つ `Main.tz` に、ディスクの本文の `E1002`（`unknown value 'Helper'`）が入り、直して保存しても残った（`verify-vscode-e2e/delete.out` の D2・D3）。
- 写しが入るのはエラーだけ。警告だけなら `tsuzuri test --list --json` は終了コード 0 で（stderr には警告が出る）、拡張は終了コードが 0 のとき stderr を読まない（`vsc/src/testing.ts:47`）。エラーと警告が両方あれば、stderr にはエラーだけが出る。
- 言語サーバーはケーパビリティを検査しない（`_tsuzuri/language-reference/organizing-tsuzuri/packages.md:363`）。
  そのため、`E1036` がエディターに出るのはこの写しを通してだけで、`Tsuzuri.toml` を直して保存しても残る（`verify-task-matcher/e2e/e2e-extra.log` の X3・X4）。

消えるのは、同じ根の次の `publishDiagnostics` だけです。後の失敗した発見（そのときのエラーで置き換わる）、1 件以上のテストを選んだテストの実行、テストのデバッグ、Debug Project のどれかです。
テストのないプロジェクトでは、Test Project も消せない。選んだテストが 0 件だと `run` が `publishDiagnostics` の前に `continue` するため（`vsc/src/testing.ts:192-193`、`verify-task-matcher/e2e/e2e-discovery.log` の D8）。

### 原因 2（主因・警告）: Run・Build・Check のタスクの problem matcher のマーカーが、次のタスクまで残る

確認済み。再現 2:

1. `Main.tz` に `let unused = 7` を足す（`W1001`。言語サーバーの警告が出る）。
2. ▷（Run Project）を押す。端末に `<絶対パス>/Main.tz:<行>:<桁>: warning[W1001]: ...` が出て、問題パネルに幅 0 の `Tsuzuri` の `W1001` が増える。
3. その行を消して保存する。`tsuzuri` の警告は消えるが、`Tsuzuri` の `W1001` は残る。言語サーバーを再起動しても残り、次に Tsuzuri のタスクをどれか流すと消える。

```text
SNAPSHOT 3a.1 after check task (warning state): Main.tz [tsuzuri] W1001 Warning @3:8-3:14 | Main.tz [Tsuzuri] W1001 Warning @3:8-3:8
SNAPSHOT 3a.2 fixed+saved after check task +15s: Main.tz [Tsuzuri] W1001 Warning @3:8-3:8
M7 after restartLanguageServer +5s: Main.tz src=Tsuzuri W1001/W @3:8-3:8 (zero-width)
M3 after a clean Run task: (none)
```

（`target/scratch/g39/vscode-e2e/tasks.out`、`verify-vscode-e2e/task.out`、`verify-task-matcher/e2e/e2e-tasks.log` の T1〜T8）

仕組み:

- `vsc/src/workflow.ts:117-119` が、拡張のすべてのタスク（build・check・run・test・debug・wasm）に problem matcher `$tsuzuri` を付ける。
- VS Code は、problem matcher のマーカーを、同じ owner のタスクが次に終わるまで持つ。文書の編集・保存・クローズでは消さない。拡張には、ほかの owner のマーカーを消す API がない。
- 正規表現は CLI の行（`src/diagnostic.rs:497-498` の `{path}:{line}:{column}: {severity}[{code}]: {message}`）と合い、パスは絶対パスなので、マーカーは正しいファイルに付く。
  行と桁だけなので範囲は幅 0 になる。

起きる条件:

- 診断を出す Run・Build・Check・Wasm のタスクすべて（`test` のタスクは Tasks: Run Task からだけ。Test Project と Testing ビューはタスクを使わない）。
- 警告が残る主な経路はこれと原因 3 です（原因 1 の発見は警告の写しを入れない）。報告にある「警告が消えない」は、この 2 つから来る。
- エラーがあれば、1 つのエラーが 3 件並ぶことがある。言語サーバーの範囲つきの 1 件、発見の写し（T1 ではウィンドウを開いた直後の発見が入れたもの。未保存のエラーを Run の `saveAll` が保存したときも入る）、タスクの幅 0 のマーカーです（`verify-task-matcher/e2e/e2e-tasks-run1.log` の T0・T1）。
- owner が共通なので、別のプロジェクトのタスクでも消える（`e2e-misc.log` の M3）。残る原因にはならない。

### 原因 3: テストの実行・テストのデバッグ・Debug Project の結果が、直した後も残る

確認済み（テストのデバッグは、同じ `publishDiagnostics` を呼ぶことをコードで確認しただけ）。

```text
SNAPSHOT 4a.1 after test run (warning state): Main.tz [Tsuzuri] W1001 Warning @3:8-3:14 | Main.tz [tsuzuri] W1001 Warning @3:8-3:14
SNAPSHOT 4a.2 fixed+saved after test run +15s: Main.tz [Tsuzuri] W1001 Warning @3:8-3:14
SNAPSHOT 4b.3 after a passing test run: (none)
G2 after Debug Project in the warning state: Main.tz src=Tsuzuri W1001/W @3:8-3:14 || Main.tz src=tsuzuri W1001/W @3:8-3:14
G3 fixed and saved +10s: Main.tz src=Tsuzuri W1001/W @3:8-3:14
```

（`target/scratch/g39/vscode-e2e/testrun.out`、`verify-vscode-e2e/debug.out`。Debug は同梱の CodeLLDB で動かした）

- `vsc/src/testing.ts:200`・`:265` と `vsc/src/workflow.ts:177` は、コンパイルの stderr を毎回 `tsuzuri-build` に入れる。テストが通っても警告は stderr に出るので、警告も入る。
  直して保存すると成功した発見が動くだけで、何も消えない。
- エディターの右上の 3 つのボタンは、どれも直した警告を残しうる。Run と Build は原因 2、Debug はこれです。
- UTF-8 の BOM で始まるファイルでは、1 行目の写しが 1 桁右にずれる。`vsc/src/workflow.ts:84-91` がディスクのバイト列（BOM を含む）で位置を数えるが、VS Code は BOM を除いた本文を言語サーバーへ渡すため（`verify-build-collection/vscode/result-bom.json`: `Tsuzuri E0002 0:16-0:17` と `tsuzuri E0002 0:15-0:16`）。
- ワークスペースのフォルダーを外しても、その根の項目は残る（`vsc/src/testing.ts:115-126` は `diagnosticsByRoot` を消さない）。

### 原因 4: 言語サーバーが、前に診断を送った URI を覚えていない

共通の原因は、`Session` が「どの URI に空でない診断を送ったか」を持たないことです。
送る先は、読み込めたときは今のプロジェクトの利用者のソース（`src/lsp.rs:396-408`）、読み込めないときは `error.path` の 1 か所（`src/lsp.rs:374-380`）だけで、それ以外は `didClose` で消えるのを当てにしている。
次の場合、言語サーバー自身の `tsuzuri` の診断が残る。「Restart Language Server」では消える（クライアントの停止が集まりを捨てる。`vsc/node_modules/vscode-languageclient/lib/common/client.js:1012-1014`）。

#### 4a. 開いていないファイルが、削除・名前の変更・移動でプロジェクトから外れた

確認済み。開いていない `Helper.tz` に別のファイルから来たエラーがある状態で、`Helper.tz` を消す:

```text
886ms publish <base>/real/proj/Helper.tz ["E1003@0:20"]
3005ms --- deleted Helper.tz on disk, didChange Main.tz (no longer uses Helper), didChangeWatchedFiles type 3
3759ms publish <base>/real/proj/Main.tz []
```

`Helper.tz` には空の一覧が送られない（`target/scratch/g39/verify-vscode-e2e/lsp-delete.out`）。
VS Code でも、名前の変更・削除の後に古いパスの `Helper.tz [tsuzuri] E1003` が残る（`vscode-e2e/remove.out` の R.3・R.5、R.6 の再起動で消える）。
同じ仕組みで、コピーを消した後の `Main copy.tz [tsuzuri] E1011`（`verify-lsp-lifecycle/e2e/copy.out`）、外した path 依存のファイル（`logs/depdrop.log`）、読めなくなったディレクトリ（`logs/dirperm.log`）も残る。
開いているファイルなら、VS Code の `didClose` で消える。

#### 4b. `Tsuzuri.toml` の誤りが、直した後も残る

確認済み。マニフェストの誤りは `src/lsp.rs:378` で `Tsuzuri.toml` へ送られるが、refresh はソース（`project.sources`）だけを回り、マニフェスト（`project.manifests`、`src/driver.rs:2763-2791`）には送らない。

```text
M.1 broken manifest saved: Tsuzuri.toml [Tsuzuri] E0002 @7:0 | Tsuzuri.toml [tsuzuri] E0002 @0:0
M.2 fixed manifest saved +5s: (same two)
M.3 after an edit of Main.tz +5s: (same two)
```

（`target/scratch/g39/verify-lsp-lifecycle/e2e/manifest.out`、`logs/manifest.log`）
範囲はいつも 0:0 で、CLI の `--json` の span（この例では 7:0）を捨てている。

#### 4c. 読み込みに失敗している間、ほかのファイルの診断が凍り、失敗の理由が隠れることがある

確認済み。

- 凍る: 読み込みに失敗すると、`refresh` は `error.path` に 1 件送って返る（`src/lsp.rs:376-380`）。ほかのファイルには何も送らないので、前の解析の診断が残る。
  マニフェストが壊れている間に `Main.tz` のエラーを直しても、`Main.tz` の `E1003` は消えない（`logs/freeze.log`）。
- 隠れる: 失敗の理由のファイルを一度開いて閉じていると、`Session::publish` が閉じた文書への空でない診断を捨てる（`src/lsp.rs:347`）。利用者には凍った古いエラーだけが見え、理由が出ない。
  `Main.old.tz`（stem に `.` を含むので `E1011`）を開いて閉じた後に `Main.tz` を直すと、`Main.tz [tsuzuri] E1003` が残り、理由の `E1011` は原因 1 の写しでしか見えなかった（`verify-lsp-lifecycle/e2e/hidden2.out` の H.2・H.3）。
- 読み込みの失敗になるもの: マニフェストの誤り、stem に `.` を含むファイル（`Main.old.tz`・`foo.test.tz`）、ソースのシンボリックリンク、読めないディレクトリ、UTF-8 でないファイル、4 MiB を超えるファイル。

#### 4d. 根の文書をすべて閉じると、ディスクの変更を解析しない

確認済み。`workspace/didChangeWatchedFiles`（`src/lsp.rs:487-493`）は、開いているバッファの根だけを schedule する。
利用者が根の `.tz` のエディターをすべて閉じても、クライアントは止まらない。その後の `git checkout` や外のツールでの修正は解析されず、前の診断が残る（`verify-lsp-lifecycle/e2e/noopen2.out` の N.2: ディスクで直した後も `Helper.tz [tsuzuri] E1003`）。

#### 4e. 開いていないファイルの URI が、クライアントの表記と違うことがある

確認済み（シンボリックリンクと NFC・NFD）。`document_path` はクライアントの URI の親を正準化し（`src/lsp.rs:257-265`）、`Project::load_with_overlays` も根を正準化する（`src/driver.rs:2396-2412`）。
開いていないファイルへは `file_uri(正準化したパス)`（`src/lsp.rs:350-355`・`178`）で送るので、ワークスペースのパスにシンボリックリンク（`/tmp` は `/private/tmp`、リンクにした作業ディレクトリ）か Unicode の正規化の違いがあると、最初の診断が別の URI に付く。
そのファイルをワークスペースのパスで開くと、以後はバッファの URI へ送り、最初の URI には二度と送らない。

```text
SNAPSHOT A.1 cross-file error, Helper closed (unsaved): <real>/Helper.tz [tsuzuri] E1003 Error @0:20-0:31
SNAPSHOT A.2 Helper opened through the workspace path +3s: <real>/Helper.tz [tsuzuri] E1003 Error @0:20-0:31 | Helper.tz [tsuzuri] E1003 Error @0:20-0:31
SNAPSHOT A.3 fixed (unsaved) +5s: <real>/Helper.tz [tsuzuri] E1003 Error @0:20-0:31
SNAPSHOT A.4 after restartLanguageServer +5s: (none)
```

（`target/scratch/g39/vscode-e2e/alias-symlink.out`。NFD は `alias-nfd.out`）
大文字・小文字だけの違いは残らない。macOS と Windows の VS Code は、大文字・小文字だけが違う URI を 1 つにまとめる（`alias-case.out`）。
`docs/architecture.md` の言語サーバーの節の「クライアントから受信したドキュメント URI の表記揺れはそのまま内部で保持され、診断情報や定義ジャンプの応答において正確にクライアントへ返却されます」は、開いていないファイルについては正しくない。

#### 4f. クライアントの根の外にある path 依存のファイルの診断が、二重に出て片方が残る

確認済み。アプリのサーバーは、根の外の依存（`../lib`）のファイルにも診断を送る。
依存のファイルを開くと、拡張はその根（`lib/`）の別のクライアントを起動する。そこで直しても、アプリのサーバーには変更が届かない（watcher と documentSelector はアプリの根の下だけ。`vsc/src/extension.ts:37`・`42`）。

```text
D.1 lib error: lib/Calc.tz [tsuzuri] E1003 @2:20 | lib/Calc.tz [tsuzuri] E1003 @2:20
D.3 lib fixed and saved +5s: lib/Calc.tz [tsuzuri] E1003 @2:20
```

（`target/scratch/g39/verify-lsp-lifecycle/e2e/dep.out`）根の中の依存なら、両方のクライアントが消すので残らない。

### 原因 5（遅れ）: 言語サーバーが古い要求や取り消された要求を計算している間、直した本文の診断が出ない

確認済み。残り続けることはない（実際の VS Code の 20 回の実行のすべてで、最後の版の診断は届いた）。ただし、直した後の数秒、大きいプロジェクトでは 20 秒近く、直す前の診断が見え続ける。
計測の harness と記録は `target/scratch/g39/lsp-latency-robustness/` と `target/scratch/g39/verify-lsp-latency-robustness/` にある。時間は Apple Silicon の macOS で、ほかの作業と共有した機械の値です（性能の主張ではなく、仕組みの裏付け）。

- **1 回の refresh の重さ**: `didChange` から同じ版の `publishDiagnostics` まで（200 ms の debounce を含む。release の中央値）は、1 ファイルで 745 ms、21 KB で 821 ms、70 KB で 1009 ms、6 ファイル 215 KB で 1929〜2126 ms。
  時間の約 95 % は型検査で、エディターの経路はすべての std モジュールを毎回検査する（`src/driver.rs:2449`、D-40）。同じ 1 ファイルの `tsuzuri check .` は 0.09〜0.17 秒です。
- **debounce がほとんど効かない**: VS Code は打鍵の直後に `textDocument/completion` などを送り、それらの処理（`src/lsp.rs:574`・`651`・`676`・`709`・`724`・`736`・`746`）が `Session::document` を通してすぐに refresh する。
- **版をまとめない**: `serve` は受け取った順に 1 つずつ処理する（`src/lsp.rs:2864-2944`）。要求が挟まると、待っている途中の版を 1 つずつ解析して送る。電球を切った VS Code で 215 KB のプロジェクトを 1 秒に 1 回直すと、版ごとの診断が約 1.7 秒おきに届き、最後の版は最後の編集の 5.3 秒後だった（`vscode/run-big-prefix-1000-nolightbulb`）。
- **電球のコードアクション**: VS Code はカーソルが止まるたびに、`only` なしで `triggerKind` 2（Automatic）の `textDocument/codeAction` を送る。
  `Session::code_actions`（`src/lsp.rs:1106-1125`）は、範囲の診断の修正案ごとに `fix_edit` でプロジェクトを読み込み直して解析し（`src/lsp.rs:1187-1225`・`1269-1316`）、文書に `certain` の修正案が 1 つでもあれば、カーソルの位置に関係なく `fix_all`（`src/lsp.rs:1130-1184`）で `tsuzuri fix` と同じラウンドを回す。
  1 回の費用は、1 ファイル（`W1001` が 3 つ）で診断のない行が約 0.9 秒、診断の上が約 1.4 秒。163 KB で 1.8 秒と 2.9 秒、215 KB で 2.5 秒と 3.9 秒。
  ところが VS Code 1.103.2 は、自動の要求の結果から source の action を捨てる（`workbench.desktop.main.js` の `xes` の `!i.includeSourceActions&&t&&Fn.Source.contains(t)`。電球の要求は `filter` を持たない）。電球のための `fix_all` の計算は、すべて捨てられている。
- **取り消しが効かない**: 取り消しを確かめるのは処理の前と後だけ（`src/lsp.rs:2924-2944`）なので、計算の途中で取り消された要求も最後まで計算し、-32800 で答える。
  1 つの修正の後の遅れは、1 ファイルで電球ありが 1.57 秒・なしが 0.58 秒、163 KB で約 3.0 秒と 1.25 秒だった（`verify-lsp-latency-robustness/vscode/s600-*`・`big2-single-*`）。
- **受信の詰まり**: 受信のスレッドは、読んだときにだけ取り消しを記録し（`src/lsp.rs:2834-2842`）、容量 8 の `sync_channel`（`src/lsp.rs:2827`）への送信で止まる（`2844`）。
  忙しい処理の後ろに 9 個以上の通知や要求が並ぶと、後の `$/cancelRequest` を読めず、取り消された要求を普通に計算して答える（`cancel-queue-release.log`: 間に 8 個で計算した後に -32800、9 個以上で結果を返した）。
  refresh とコードアクションの時間が編集の間隔に近いとこれが連鎖し、215 KB のプロジェクトを 1 秒に 1 回直すと、最後の診断は最後の編集の 8.8〜20.4 秒後になった（電球なしは 5.3 秒）。163 KB・9 ファイルの別のプロジェクトでは連鎖せず、電球の有無で差がなかった（2.3 秒）。閾値のある現象で、20 秒は典型ではない。
- **取り消しの id が溜まる**: 答えた後に読んだ取り消しの id は集合から消えない（消すのは同じ id の要求の前後の確認だけ）。集合は 1024 個で新しい id を拒む（`src/lsp.rs:2840`）ので、そこから先は取り消しがまったく効かない。
  215 KB のプロジェクトを編集し続けると 1 秒に約 0.4 個溜まった（約 40〜45 分で上限。外挿で、観測はしていない）。
- vscode-languageclient は、`publishDiagnostics` の版を見ずに順に適用する（`client.js:1073-1083`）。古い版の診断も、届いた順に表示される。

### 原因ではなかったもの

- 言語サーバーが、編集したファイルの診断を消さない: 未保存でも保存しても消える（`vscode-e2e/lsp.out` の 1a.3、1b.3 は修正の保存から 649 ms で消えた）。
- 編集したファイルが原因で別のファイルに出たエラーが残る: 別のファイルが開いていてもいなくても、約 0.6 秒で消える（`vscode-e2e/multi.out`）。
- 小さいプロジェクトで打鍵だけで遅れる: 2 ファイルで `type`・`deleteLeft` で実際に打つと、エラーは 333〜401 ms で出て、消すと最後のキーから 567〜621 ms で消える（`vscode-e2e/typing.out`）。遅れが大きくなるのは原因 5 の条件のときです。
- 入力の途中の状態で言語サーバーが panic・スタックあふれ・終了し、クライアントが古い診断を持ち続ける: 起きない。例と fixture の 1724 通りの切り詰め・括弧の不釣り合い・書きかけの文字列と補間を、ほかの要求と一緒に送っても、プロセスは一度も終わらなかった（`lsp-latency-robustness/fuzz-large.log`・`fuzz-examples.log`。入れ子は 128 段で `E0002` になる）。
- 最後の版の診断が、次の編集まで送られない: 起きない（上記）。
- 原因 1 の発見が警告の写しを入れる: 入れない（上記）。
- problem matcher の正規表現が CLI の行と合わない、パスが相対で別の URI に付く: 合っていて絶対パス。例外は [wasm] の検査の 1 か所だけ（対象外を参照）。
- PB06 の stat のキャッシュが古い本文を返す、言語サーバーがバッファでなくディスクを読む: どちらも起きない（`lsp-lifecycle` の h1、`tests/lsp_sessions.mjs` の 2 秒の窓の検査と同じ結果）。
- publish の `version` で vscode-languageclient が診断を捨てる: 捨てない。
- 大文字・小文字だけが違うワークスペースのパス: 上記 4e。

### 関連する観測（このチケットの対象外）

- **入れ子のパッケージを覆う根のクライアント**: ワークスペースのフォルダー自体がプロジェクトでなく（直下に `Tsuzuri.toml` も `Main.tz` もない）、その下にパッケージがあると、パッケージの外の `.tz` を開いたときにフォルダーの根のクライアントが起動し、その documentSelector（`<根>/**/*.{tz,tt,tc}`）がパッケージのファイルも拾う。
  マニフェストを読まないフォルダーとしての解析になるので、マニフェストの名前空間に頼るファイル（ヘッダーのない `app/Util.tz` を `App::Util` で参照）に、直しても消えない偽のエラーが出る（`verify-lsp-lifecycle/e2e/fallback.out`、`logs/noheader.log`）。
  このリポジトリ自身を開いて `std/` や `tests/` の `.tz` を開くと、根のクライアントの読み込みが `E1017`（`module discovery exceeds 1024 directories or 16 path segments`）で毎回失敗し、それがディレクトリの URI に付く（`verify-lsp-lifecycle/logs/repo-root.log`）。
  どのクライアントがどの文書を受け持つかの問題で、診断の寿命の問題ではないので、[G40](_completed/G40-fallback-root-client.md) で扱う。
- **4 MiB を超える `didChange`**: 拒否して前のバッファを残す（`src/lsp.rs:532-537`）ので、前の診断も残る。縮めると戻る。直すと `tests/lsp.rs` の `rejects_documents_over_the_source_limit` の期待が変わる。
- **[wasm] の検査のエラーの相対パス**: `src/main.rs:4167-4175` が `Tsuzuri.toml:1:1` と相対パスで出すので、絶対パスの problem matcher が `file:///Tsuzuri.toml` に付け、次の Wasm のタスクまで残る（`verify-task-matcher/e2e/e2e-misc.log` の M5〜M7）。D3 で matcher を外せばマーカーにはならないが、ほかの診断と同じ絶対パスで出すのが正しい。
- **1 回の refresh の費用そのもの**: エディターの経路はすべての std モジュールを毎回検査し（D-40）、1 ファイルのプロジェクトの refresh の標本の 49 % が `check::warnings::unused_private`（`src/warnings.rs:286`、`src/check.rs:9525` から）で、std を含むすべての関数を走査している（`verify-lsp-latency-robustness/sample-hello-debug.txt`）。
  CLI の `check` との差（約 5 倍）を縮めるのは性能の作業で、[PB08](../_perfs/PB08-lsp-refresh-cost.md) で扱う。Phase 3 はこの費用を変えず、無駄な計算と待ちだけを除く。
- **エラーがある間、ほかのファイルの警告が消える**: 解析が失敗すると `analyze`（`src/lsp.rs:1549-1553`）は警告を返さないので、どこかにエラーがある間はプロジェクトのすべての警告が消え、直すと戻る。直した警告が戻ることはない。残る原因ではない。
- **超線形の入力**: 1 つのブロックに `let v = v + 1;` を 10000 個並べると refresh が 16.7 秒（`tsuzuri check` も 10〜21 秒）、要素 30000 の配列リテラルの整形が 8 秒、10 万個の `else if` などは 120 秒で答えなかった。その間はどのファイルの診断も出ない。機械が生成した大きなブロックだけの問題で、別に扱う。
- 実行中のテスト・Debug のビルドが、修正の保存の前にディスクを読んでいると、修正の後に古い結果を入れうる（コードを読んだだけで、再現していない。落とし穴を参照）。
- 確かめていないこと: Linux での大文字・小文字だけの名前の変更（別の URI になるので、4a と同じく残るはず）、`tsuzuri serve`（常駐ビルドサーバー）が動いているときの `check`・`build` の転送（`src/main.rs:4124-4131`）、CR だけの改行の文書への要求が `-32602`（`position is outside the document`）になったこと（`lsp-latency-robustness/deep-release.log`）。
- run・build・test のタスクの後、プロジェクトの根に 0 バイトの `a.o` が残った（`vscode-e2e/runs/tasks/real/project/a.o` など。調べていない）。

## 仕様（直した後の振る舞い）

### Phase 1: 拡張

1. 構文・型・所有権・警告などの前段の診断は、言語サーバー（source `tsuzuri`）だけが出す。
2. 裏で動くテストの発見（watcher の事象、`resolveHandler`・`refreshHandler`）は診断を出さない。発見の失敗は、今と同じく Testing ビューのプロジェクトの項目のエラーに出す（`vsc/src/testing.ts:75`）。
   利用者が明示したテストの実行とテストのデバッグ（`run`・`debug`）は、最初に `discover` を待ち（`vsc/src/testing.ts:191`・`249`）、ケーパビリティの検査（`E1036`）などで発見が失敗するとそこで終わる。そのときの発見の失敗は、その実行の結果として 3 の規則（変更の番号の確かめ、言語サーバーとの重複の除外、変更で消す）で出す。
3. テストの実行・テストのデバッグ・Debug Project のビルドの診断（source `Tsuzuri`）は、その実行の結果として出し、その根のプロジェクトが変わったら消す。
   変わったとは、根の下の文書のエディターでの編集、根の下の `.tz`・`.tt`・`.tc`・`.toml` の作成・削除、根の下のフォルダーの削除・移動、実行を始めた後の変更（保存を含む）、ワークスペースのフォルダーの削除のどれかです。
   実行の途中で根が変わったら、その実行の結果は入れない。
   結果が根の外のファイル（根の外の path 依存のソースや `Tsuzuri.toml`）を指すときは、その根は根の外の依存の入力に依る。依存のどのファイルが結果に効いたかは分からない（`lib/Calc.tz` のエラーの原因が `lib/Shapes.tz` の型にあることもある）ので、保守的に、根の外のソースの変化をすべてその根の「変わった」に数える。
   そうした結果を入れるとき、実行の開始より後に、根の外のどれかの Tsuzuri の文書がエディターで編集されたか、ワークスペースの watcher が根の外の `.tz`・`.tt`・`.tc`・`.toml` の作成・削除・変更を届けていれば入れない（拡張は、根の外の変化の最後の時刻を 1 つ記録する）。
   入れた後も、根の外の Tsuzuri の文書の編集と、根の外の `.tz`・`.tt`・`.tc`・`.toml` の watcher の事象で、その根の項目を消す。根の外を指さない結果は、今までどおり根の下の変化だけで判定する。
4. 3 の診断のうち、言語サーバーが同じ URI・コード・範囲の開始で出しているものは出さない（重複させない）。言語サーバーが出さない診断（`E1036`、`E2xxx`、言語サーバーが読み込めない状態の診断）は残る。
5. 3 の位置は、VS Code の文書と同じ本文（BOM を除く）で数える。
6. 拡張のタスク（build・check・run・test・debug・wasm）は problem matcher を持たない。結果は端末に出し、`<path>:<line>:<column>:` は端末のリンクとして開ける。
   `package.json` の `$tsuzuri` の定義は残し、利用者の `tasks.json` から使える。そのマーカーが次のタスクまで残るのは VS Code の仕様で、`vsc/README.md` に書く。

### Phase 2: 言語サーバー

7. 根ごとに、直前の refresh で空でない診断を送ったパスと、そのとき使った URI を持つ。次の refresh で、そこにあって今回は空でない診断を送らないパスには、記録した URI へ `[]` を送る。同じパスへ前と違う URI で送るときは、前の URI にも `[]` を送る。読み込めたときも読み込めないときも同じ。
   これで、プロジェクトから外れたファイル、直した `Tsuzuri.toml`、外した依存の診断が消える。
8. 読み込めないとき: エラーを `error.path` へ送る。その文書が閉じていても送る（閉じた文書への抑止を読み込みのエラーには使わない）。範囲は、エラーの span があればその位置、なければ 0:0。
   ほかのファイルの前の解析の診断は、7 によって消える。
9. `workspace/didChangeWatchedFiles` は、変わった各 URI のパスを、存在する最も近い祖先のディレクトリで正準化し（`document_path` と同じ。消したファイルやディレクトリは、残っている親まで遡る）、それが入るクライアントの根を schedule する。
   どのクライアントの根にも入らない URI では、親のディレクトリを根にしない。加えて、開いているバッファの根（今と同じ）と、7 の記録を持つ根を schedule する。
10. 開いていないファイルの URI は、クライアントの表記で作る。`initialize` の `rootUri`・`workspaceFolders` の URI と、それを正準化したパスの組を持ち、正準化したパスの接頭辞をクライアントの根の URI に置き換える。
    どの根にも入らないパスは、今と同じく `file_uri` で作る。
11. クライアントの根（refresh した根のディレクトリ）の外にある利用者のソースには、解析の診断を送らない。そのファイルを開けば、拡張がそのファイルの根のクライアントを起動し、そのクライアントが送る。
    読み込みのエラー（8）は例外で、`error.path` が根の外（根の外の依存の `Tsuzuri.toml` やソース）でも送る。送らないと、8 でほかの診断を消した根に何も出ない。根の外のファイルの変更はこのクライアントに届かないので、そのエラーは根の次の refresh まで残る。
    拡張は、フォルダーの削除・移動でも `didChangeWatchedFiles` が届くよう、根の `**` の削除を見る watcher を足す（VS Code はフォルダーの中のファイルの事象を親の 1 件にまとめ、`**/*.{tz,tt,tc,toml}` には届かない）。

### Phase 3: 言語サーバーの待ち行列と取り消し

12. 要求を処理する前に、届いている入力をすべて待ち行列へ取り込む。要求の文書に、後から届いた `didChange`・`didClose` があれば、その要求は計算せず、`ContentModified`（-32801）で答える。
    計算する要求の前に届いた text sync は、どの文書のものもすべて届いた順に反映してから、その要求を処理する（`didChange(A,v2)`→`hover(B)`→`didChange(A,v3)` なら、`hover(B)` は A の版 2 を見る。`Session::document` はプロジェクト全体の状態を読み、別の文書の要求やファイルをまたぐ rename も A の本文を使うため）。
    同じ文書の `didChange` をまとめて最後の 1 つだけを反映するのは、その間に計算する要求がないときだけ（全文同期なので、それで足りる。版が増えていることの検査は今と同じ）。
13. `$/cancelRequest` は、待ち行列の長さに関わらず、その要求を始める前に効く。主ループの計算中も受信のスレッドは背圧の上限まで読み続けるので、処理中の要求の取り消しも 14 の確認で効く。
    待ち行列にも処理中にもない id の取り消しは捨て、答えた id は集合から消し、件数の上限で新しい取り消しを拒まない。
14. `textDocument/codeAction` は、修正案の検証の間（修正案ごと、fix all のラウンドごと）に取り消しを確かめ、取り消されたらそこで -32800 を答える。
15. `context.triggerKind` が 2（Automatic）で `context.only` がない要求には、`source.fixAll.tsuzuri` を計算しない。`triggerKind` が 1（Invoked）かない要求と、`only` に `source` 系がある要求は今と同じ。
    クイックフィックスの検証（適用後の再検査で、その診断のコードが減り、どのコードのエラーも増えない修正案だけを出す）は変えない。

## 設計

### Phase 1 の変更

| ファイル | 関数・場所 | 変更 |
| --- | --- | --- |
| `vsc/src/testing.ts` | `discover`（46〜50 行） | `publishDiagnostics` を呼ばない。失敗は `throw` して `project.error` に出す（今と同じ）。`run`・`debug` から呼んだときは、失敗の stderr を呼び出し側へ返し、`run`・`debug` が実行の結果として `publishDiagnostics` へ渡す（発見を始める直前の変更の番号を使う）。裏の発見（`changed`・`resolveHandler`・`refreshHandler`）は出さない |
| `vsc/src/workflow.ts` | `publishDiagnostics`（75〜108 行） | 実行の開始時刻を受け取り、根ごとに記録する。言語サーバーの診断（`vscode.languages.getDiagnostics(uri)` の source `tsuzuri`）と URI・コード・範囲の開始が同じ記録を除く。位置は BOM を除いた本文で数える |
| `vsc/src/workflow.ts` | 新しい購読 | `onDidChangeTextDocument`（`contentChanges` が空でないもの）、watcher の作成・削除と、その根で最後に始めた実行の開始より新しい mtime の変更、`onDidChangeWorkspaceFolders` の削除で、その根の項目を消し、根ごとの変更の番号を増やす。フォルダーの削除・移動は、VS Code が中のファイルの事象を親の 1 件にまとめるので `**/*.{tz,tt,tc,toml}` には届かない。根ごとに `new vscode.RelativePattern(root, '**')` の削除だけを見る watcher も足す（名前が `.` で始まる所と `target/tz/` の下は、モジュールの発見が読まず、拡張のランナーと知識グラフ（G27）が実行のたびに書き換えるので除く） |
| `vsc/src/workflow.ts` | `task`（110〜123 行） | `new vscode.Task(..., '$tsuzuri')` の最後の引数を `[]` にする |
| `vsc/src/testing.ts`・`vsc/src/workflow.ts` | `run`（196〜200 行）、`debug`（263〜265 行）、`debugConfiguration`（172〜177 行） | コンパイラを起動する直前の時刻と根の変更の番号を `publishDiagnostics` へ渡す。結果を入れるときに番号が変わっていれば入れない |
| `vsc/src/core.ts` | 新しい純粋な関数 | バイトの offset から VS Code の位置への変換（BOM・CRLF・UTF-16）、重複の判定、パスがどの根に入るかの判定。`vsc/src/test/core.test.ts` で単体に検査する |

`publishDiagnostics` の消し方は、根ごとの今の `diagnosticsByRoot` を使う。項目を消すときは、その根の URI をすべて消す（ファイル単位にしない。1 つのファイルの変更は、ほかのファイルの診断も変えるため）。

### Phase 2 の変更

| ファイル | 関数・場所 | 変更 |
| --- | --- | --- |
| `src/lsp.rs` | `Session` | `published: BTreeMap<PathBuf, BTreeMap<PathBuf, String>>`（根 → 空でない診断を送ったパスと、そのとき使った URI）、`client_roots: Vec<(PathBuf, String)>`（正準化した根 → クライアントの根の URI）を足す |
| `src/lsp.rs` | `Session::refresh`（366〜427 行） | 送ったパスと URI を集め、前の記録にあって今回送らないパスには記録した URI へ `[]` を送り、同じパスへ前と違う URI で送ったときは前の URI にも `[]` を送ってから、記録を置き換える。読み込みの失敗の経路（374〜380 行）も同じ。根の外のソースには解析の診断を送らない（D9。読み込みのエラーは例外） |
| `src/lsp.rs` | `Session::publish`（341〜364 行） | 読み込みのエラーは `closed` でも送る（引数か別の関数で区別する）。URI はバッファ → `closed` → クライアントの根の表記 → `file_uri` の順 |
| `src/lsp.rs` | 読み込みの失敗の診断（378 行） | `error.diagnostic.span` を `PositionMapper` で範囲にする。本文を読めなければ 0:0 |
| `src/lsp.rs` | `initialize`（440〜455 行） | クライアントの根の URI と正準化したパスの組を記録する |
| `src/lsp.rs` | `workspace/didChangeWatchedFiles`（487〜493 行） | `params.changes` の各 URI を 9 の規則で正準化してから `project_root` に渡し、クライアントの根に入るものだけを schedule する。開いているバッファの根と `published` の根も schedule する。`uri_path` の値をそのまま渡さない（正準化した根と前方一致しないと、`project_root` は親のディレクトリを根として返す。Windows の `c:/...` と `\\?\C:\...`、シンボリックリンクの根で起きる） |
| `src/lsp.rs` | `Session::uri`（800〜805 行） | 10 と同じ規則でクライアントの表記を返す（定義・参照の応答も同じ URI になる） |
| `vsc/src/extension.ts` | `open` の `synchronize.fileEvents`（37〜44 行） | 根の `**` の削除だけを見る watcher を足す（除く場所は Phase 1 と同じ）。フォルダーを消したり移したりしたときにも `didChangeWatchedFiles` が届くようにする |

### Phase 3 の変更

| ファイル | 関数・場所 | 変更 |
| --- | --- | --- |
| `src/lsp.rs` | `serve`（2822〜2962 行） | 容量 8 の `sync_channel` をやめ、受信のスレッドが読み続けられるようにする。背圧は、読んで主ループがまだ処理していないメッセージの本文の合計（主ループの待ち行列の分を含む）にかけ、64 MiB（`MAX_MESSAGE` の 4 倍）を超えたら受信のスレッドは減るまで読むのを待つ。主ループは毎回チャンネルを `try_recv` で空にして自前の待ち行列へ移す。同じ文書の `didChange` は、その間に計算する要求がないときだけまとめて最後の 1 つを残し、計算する要求の前に届いた text sync はすべて届いた順に反映してから要求を処理する（仕様 12）。要求の前に、同じ文書の後の text sync が待ち行列にあれば -32801 で答える |
| `src/lsp.rs` | 受信のスレッド（2830〜2858 行）と取り消しの集合（2828・2924〜2944 行） | 受信のスレッドは、読んだ `$/cancelRequest` の id を今と同じく共有の集合に入れる（処理中の要求の途中の確認のため）。主ループは待ち行列へ移すときに照合し、待っている要求なら始めずに -32800 で答える。待ち行列にも処理中にもない id（答えた後の取り消し、存在しない id）は、そのとき集合から捨てる。答えた id も消す。件数の上限は置かない（大きさは上の背圧で抑えられる） |
| `src/lsp.rs` | `code_actions`・`quick_fixes`・`fix_all`・`fix_edit`（1106〜1330 行） | 取り消しを確かめる関数を受け取り、修正案ごととラウンドごとに呼ぶ。`triggerKind` 2 で `only` がなければ `fix_all` を呼ばない |
| `src/fix.rs` | `run`（637 行〜） | ラウンドの間に取り消しを確かめる関数を受け取る（今の `analyze: &mut dyn FnMut(&Project) -> Analysis` では途中で止められない） |

待ち行列と取り消しの集合の大きさは、上の 64 MiB の背圧で抑える（今の容量 8 の `sync_channel` は 8 × 16 MiB を守っていたが、主ループの計算中に 9 個目より後の取り消しを読めなかった）。
解析を別のスレッドで動かして古い解析を捨てる案は、このチケットでは採らない（対象外）。

## 実装手順

各段で、先に回帰テストを書き、HEAD で落ちることを確かめてから直す。

### Phase 1（拡張）

#### 段 1: テストの発見の写しをやめる

- 変更: `vsc/src/testing.ts` の `discover`。
- 確認: `vsc/src/test/extension.test.ts` に次を足す。エラーを保存し、`api.testing.roots.get(root)!.error` が入る（保存の後の発見が失敗した）まで待つ。直して保存し、`error` が `undefined` に戻る（直した後の発見が成功した）まで待つ。その時点で source `Tsuzuri` の診断がない。
  発見の終わりを `error` で待つので、待ち時間に関わらず HEAD で落ち、直した後に通る（既存のテストも `api.testing.roots` を待っている。`vsc/src/test/extension.test.ts:124-128`）。
  `cd vsc && npm run test:unit`、`npm test`（環境変数の注意は着手条件を参照）。

#### 段 2: 実行の結果を、プロジェクトが変わったら消す

- 変更: `vsc/src/workflow.ts`・`vsc/src/testing.ts`・`vsc/src/core.ts`。
- 確認: 統合テストに次を足す。言語サーバーが出さない診断（例えば `Tsuzuri.toml` の `[capabilities]` を `allow = []` にして `IO` を使う `E1036`）の状態でテストを実行すると、実行の直後に source `Tsuzuri` の `E1036` がある（`saveAll` の後で結果が消えない）。
  `E1036` は `run` の最初の `discover` が失敗して出るので、この検査は仕様 2 の「明示した実行は発見の失敗を出す」も確かめる（段 1 だけを入れた状態では落ちる）。
  本文を編集すると消え、もう一度実行して `Tsuzuri.toml` を直して保存しても消える。Debug Project の同じ検査は、デバッグを試す既存の節（`supportsDebug()` の分岐）に足す。
  言語サーバーも出す警告（`W1001`）では検査しない（段 3 の重複の除外で、実行の直後に source `Tsuzuri` の警告がなくなる）。
  根の外の依存: `app/` の `Tsuzuri.toml` が path 依存 `../lib` を持ち、`lib/Calc.tz` にエラーがある状態で `app` のテストを実行すると、`lib/Calc.tz` の source `Tsuzuri` の診断が `app` の結果として出る。
  `lib/Calc.tz` を開いて直して保存すると（`lib` のクライアントだけが変わる）、`app` の写しも消える。
  別の変種で、`lib/Calc.tz` のエラーの原因を `lib/Shapes.tz` の型にし、`Shapes.tz` だけを直して保存しても（結果の URI は `Calc.tz` だけ）、`app` の写しが消える。依存のファイルを実行の途中で編集した場合は、その実行の結果を入れない（仕様 3）。

#### 段 3: 言語サーバーの診断との重複を除き、BOM の位置を直す

- 変更: `vsc/src/workflow.ts`・`vsc/src/core.ts`。
- 確認: `vsc/src/test/core.test.ts` に、BOM・CRLF・補助平面の文字の位置と、重複の判定の単体テストを足す。統合テストで、source `tsuzuri` の `W1001` が出るのを待ってからテストを実行し、その直後に、同じ警告が source `tsuzuri` の 1 件だけであることを検査する。

#### 段 4: タスクの problem matcher を外す

- 変更: `vsc/src/workflow.ts` の `task`。
- 確認: 統合テストで、警告のある状態で check と run のタスクを流し、直して保存した後、UI のマーカーに source `Tsuzuri` のものがない。
  マーカーは `vscode.languages.getDiagnostics` ではなく `editor.action.marker.next` で巡って数える（落とし穴を参照）。既存の build・run・check のタスクの終了コードの検査は変えない。

#### 段 5: 文書

- `vsc/README.md` の「コードを書く」「実行とビルド」「テスト」: 問題パネルの診断は言語サーバーが出すこと、テストの実行と Debug の結果は次の変更まで出ること、タスクは問題パネルへ出さず端末のリンクで開けること。
- `vsc/CHANGELOG.md` の次の版の節。

### Phase 2（言語サーバー）

#### 段 6: 送った URI を覚え、送らなくなった URI に `[]` を送る

- 変更: `src/lsp.rs` の `Session`・`refresh`、`tests/lsp.rs` の helper。
- 確認:
  - `tests/lsp.rs` の `session` は入力をすべて先に書いて `serve` を最後まで動かし、根を正準化した 1 つの一時ディレクトリに固定するので、途中でディスクを変える場面・シンボリックリンクの根・根の外の依存は書けない（最後の `shutdown` は待っている refresh も捨てる。`src/lsp.rs:494-497`）。
    段 6 で、`serve` を別のスレッドで動かし、メッセージを 1 通ずつ送って出力を順に読み、その間にディスクを変えられる helper（親の一時ディレクトリの下に根と依存を置き、`rootUri` を指定できるもの）を `tests/lsp.rs` に足し、段 6〜9 の検査はそれで書く。
  - 開いていないファイルにエラーがある状態で、そのファイルを消すか名前を変えて `didChangeWatchedFiles` を送ると、古い URI に `[]` が届く。フォルダーごと消した場合も同じ。
  - `Tsuzuri.toml` を壊して直すと、`Tsuzuri.toml` に `[]` が届く。
  - 根の中の path 依存（`path = "lib"`）を項目から外して `lib/` を消すと、`lib/` のファイルに `[]` が届く（項目を外すだけだと `lib/` は根のモジュールとして読み込まれ、根の外の依存には段 9 の後は初めから送らないので、どちらもこの検査に使わない）。
  - `cargo nextest run --locked --test lsp`（`running N tests` を確かめる）。

#### 段 7: 読み込みの失敗

- 変更: `src/lsp.rs` の `refresh` の失敗の経路、`publish`。
- 確認: `tests/lsp.rs` に次を足す。マニフェストが壊れている間に `Main.tz` を直すと、`Main.tz` に `[]` が届く。
  一度開いて閉じた `Main.old.tz` があると、その URI に `E1011` が届く。マニフェストの誤りの範囲が CLI の `--json` の span と同じ位置になる。

#### 段 8: 開いているバッファがなくてもディスクの変更を解析する

- 変更: `src/lsp.rs` の `workspace/didChangeWatchedFiles`。
- 確認: `tests/lsp.rs` に次を足す。ファイルを開いて閉じた後、ディスクで別のファイルのエラーを直して `didChangeWatchedFiles` を送ると、そのファイルに `[]` が届く。
  サブディレクトリのファイルを変えて `didChangeWatchedFiles` を送っても、そのサブディレクトリを根とする解析の診断が届かない。

#### 段 9: クライアントの表記の URI と、根の外の依存

- 変更: `src/lsp.rs` の `initialize`・`publish`・`uri`・`refresh`。
- 確認: `tests/lsp.rs` に次を足す。シンボリックリンクを通した根で、開いていないファイルの診断がリンクの表記の URI に届き、正準化したパスの URI には届かない（Unix だけ。Windows はシンボリックリンクの作成に権限が要るので skip する）。
  根の外の path 依存のファイルには解析の診断が届かず、根の外の依存の `Tsuzuri.toml` の読み込みのエラーは届く。
  `windows_file_uris_round_trip_drive_letters_and_canonical_paths`（`#[cfg(windows)]`。`ci:full` の `windows.yml` でだけ動く）は変えずに通る。

#### 段 10: 文書

- `_tsuzuri/language-reference/compiler/diagnostics.md` の「LSP での表示」: 読み込みのエラーは閉じた文書にも根の外のファイルにも送ること、読み込めない間はほかのファイルの前の解析の診断を消すこと（D6）、プロジェクトから外れたファイルの診断を消すこと、根の外の path 依存のファイルには解析の診断を送らず、そのファイルを開いたときの根のサーバーが送ること（D9）、マニフェストの誤りの範囲。
  「入力を読めないときも、同じオブジェクトを 1 件出します。閉じた文書に新しい診断を送り続けることはしません」を、この振る舞いに合わせて書き直す。
- `_tsuzuri/language-reference/compiler/usage.md` の `lsp` の節: ファイルの変更の通知で、開いていない根も解析し直すこと。
- `docs/architecture.md` の言語サーバーの節: URI の表記の規則（開いていないファイルはクライアントの根の表記）と、送った URI の記録。

### Phase 3（言語サーバーの待ち行列と取り消し）

#### 段 11: 待ち行列と取り消し

- 変更: `src/lsp.rs` の `serve`、`tests/lsp.rs` の既存の scripted なテスト。
- 既存のテストの移行: `tests/lsp.rs` の `session`・`scripted` は、入力をすべて先に書いてから `serve` を動かす（`tests/lsp.rs:50-130`）。
  `completion_offers_fields_members_locals_and_keywords` のように、要求の後に同じ文書の `didChange` が続くテストは、仕様 12 でその要求が -32801 になりうる。
  そうしたテストは、段 6 で足す 1 通ずつ送る helper に移し、各応答を待ってから次の編集を送る。期待値は変えない。先送りの入力（応答を待たずに続けて送る）は、待ち行列そのものを試すテストだけに使う。
- 確認: `tests/lsp.rs` に次を足す。どれも応答の種類を見る検査で、経過時間の閾値は置かない（AGENTS.md: 共有の CI に速さの合否を置かない）。
  - refresh で忙しいサーバーに、`codeAction`、9 個以上の通知、その `$/cancelRequest` の順に送ると、`codeAction` の応答が -32800 になる（HEAD では結果が返る）。
  - 存在しない id の取り消しを 1100 個送った後でも、`didChange` の直後の `hover`（refresh を伴うので、取り消しを読む前に答え終わらない）の取り消しが効く（HEAD では 1024 個で効かなくなる）。
  - 2 つの文書 A・B（B のホバーの型が A の定義に依る）で、忙しくしたサーバーに `didChange(A, 版 2)`、`hover(B)`、`didChange(A, 版 3)` を続けて送ると、`hover(B)` は A の版 2 に基づいて答える（版 1 でも版 3 でもない）。同じ文書の `didChange` を要求をまたいでまとめる誤りを見つける。
  - `hover` の refresh で忙しくしたサーバーに、`didChange`（版 2）、`hover`、`didChange`（版 3）、`documentSymbol` を続けて送ると、2 つ目の `hover` は -32801 で答え、`documentSymbol` は結果を返し、その文書の診断は版 2 のものが届かず、版 3 の 1 回だけ届く（`session` の最後の `shutdown` は待っている refresh を捨てるので、版 3 の refresh を起こす要求が要る）。

#### 段 12: コードアクションの取り消しと `triggerKind`

- 変更: `src/lsp.rs` の `code_actions`・`quick_fixes`・`fix_all`・`fix_edit`。
- 確認: `tests/lsp.rs` に、`triggerKind` 2 で `only` のない `codeAction` が `source.fixAll.tsuzuri` を返さないこと、`triggerKind` 1 では返すことを足す。
  `code_action_fix_all_is_offered_only_when_asked_and_when_there_is_something_to_fix` など、`triggerKind` を送らない既存のテストは変えずに通る。
  `src/lsp.rs` の `mod tests` に、1 回目から取り消しを返す関数を `code_actions` に渡すと、関数がちょうど 1 回呼ばれて -32800 を返す（最初の修正案の検証と `fix_all` のラウンドを始めない）検査を足す。応答の種類だけでは、計算し終えてから -32800 を返す HEAD と区別できない。

#### 段 13: 計測

- 「現状」の原因 5 の計測を、同じ harness と同じプロジェクトで直す前と後に取り、このチケットに表で記録する。少なくとも、215 KB のプロジェクトを 1 秒に 1 回直したときの最後の編集から最後の診断まで（電球あり・なし）と、1 ファイルで 1 つ直したとき。
  GUIDE §14 のとおり、着手前の release を `target/perf/G39/tsuzuri-before` に写して比べ、ほかの作業のない機械で各組を 9 回以上測り、中央値と最小・最大を記録する（環境は §14.2）。遅れが縮まったと書くのは、この計測で示せた範囲だけにする。

#### 段 14: 文書

- `_tsuzuri/language-reference/compiler/diagnostics.md` の「LSP での表示」: `source.fixAll.tsuzuri` を返す条件に `triggerKind` を足す。
- `docs/architecture.md` の言語サーバーの節: 受信の待ち行列、取り消し、`ContentModified`。

## テスト計画

- 拡張の統合テスト（`vsc/src/test/extension.test.ts`。CI では `vscode.yml` が 5 環境で動かす）:
  保存したエラーの写し（段 1）、テストの実行と Debug の結果（段 2）、重複（段 3）、タスクのマーカー（段 4）。
  Phase 2 の後に、開いていないファイルの名前の変更・削除で言語サーバーの診断が消えること（Explorer と同じ `WorkspaceEdit.renameFile`・`workspace.fs.delete`。フォルダーの名前の変更も 1 つ）。
- 拡張の単体テスト（`vsc/src/test/core.test.ts`、`npm run test:unit`）: 位置の変換、重複の判定、根の判定。
- 言語サーバー（`tests/lsp.rs`。段 6 で足す、1 通ずつ送る helper）: 段 6〜9 の各場面。どれも、空の一覧が届くべき URI に届くことと、届くべきでない URI に診断が届かないことの両方を見る。段 11・12 の取り消し・`ContentModified`・`triggerKind`。
- 実プロセスのセッション（`tests/lsp_sessions.mjs`）: 開いていないファイルの削除と `didChangeWatchedFiles` で、古い URI に `[]` が届く場面を 1 つ足す（`cargo build` の後、`node tests/lsp_sessions.mjs target/debug/tsuzuri`）。
- 既存のテストへの影響: `tests/lsp.rs` と `tests/lsp_sessions.mjs` の期待は変えない。要求の後に同じ文書の編集を先送りで続ける `tests/lsp.rs` のテストは、段 11 で 1 通ずつ送る helper に移す（期待値はそのまま）。`vsc/src/test/extension.test.ts` の build・run・check のタスクの終了コードの検査は変えない。
- Phase 1 は `vsc/` を、Phase 2 は Windows のパスと URI の扱い（D8、9 の正準化）を変えるので、どちらの PR にも `ci:full` のラベルを付ける（AGENTS.md の CI）。Windows の Rust テストは `windows.yml`、`tests/lsp_sessions.mjs` と拡張の統合テストは `vscode.yml` だけが動かす。

## ドキュメント

- `vsc/README.md`、`vsc/CHANGELOG.md`（Phase 1）。
- `_tsuzuri/language-reference/compiler/diagnostics.md`、`_tsuzuri/language-reference/compiler/usage.md`、`docs/architecture.md`（Phase 2・3）。
- `node scripts/check-docs.mjs <変更したページ>`。
- 完了したら、`_features/README.md` の G39 の行を done にし、このチケットを `_features/_completed/` へ移す。

## 受け入れ条件

- [ ] エラーを保存し、直して保存すると、source `Tsuzuri` の写しが残らない（自動保存でも）。
- [ ] テストの実行・Debug Project の後に直して保存すると、その結果の警告・エラーが消える。実行の直後には、言語サーバーが出さない結果（`E1036` など）が出ている。実行の途中で根が変わったら、その結果は入らない。
- [ ] テストの実行の直後に、言語サーバーと同じ警告が 2 件並ばない。`E1036` と `E2xxx` は実行の結果として出る。
- [ ] Run・Build・Check のタスクの後に直して保存すると、問題パネルに source `Tsuzuri` のマーカーが残らない。端末の `path:line:column` はクリックで開ける。
- [ ] 開いていないファイルを削除・名前の変更・移動すると（フォルダーごとでも）、その古い URI の言語サーバーの診断が消える。
- [ ] `Tsuzuri.toml` を直すと、その診断が消える。マニフェストの誤りの範囲は CLI の `--json` と同じ位置。
- [ ] プロジェクトを読み込めない間、ほかのファイルに前の解析の診断が残らない。読み込みのエラーは、そのファイルを一度開いて閉じていても出る。
- [ ] 根の文書をすべて閉じた後も、ディスクの変更で診断が更新される。
- [ ] シンボリックリンクを通したワークスペースで、開いていないファイルの診断がワークスペースの表記の URI に付き、開いて直すと消える。
- [ ] 根の外の path 依存のファイルに、依存する側のサーバーが解析の診断を送らない（読み込みのエラーは送る）。
- [ ] ファイルの変更の通知が、サブディレクトリや正準化していないパス（Windows の `c:/...`、シンボリックリンク）のものでも、別の根の解析を始めない。
- [ ] 待ち行列に何個並んでいても、始める前に取り消された要求は計算せずに -32800 で答える。取り消しの id が溜まって取り消しが効かなくなることがない。
- [ ] 後の `didChange` がある文書への要求は -32801 で答え、途中の版の診断を 1 つずつ送らない。
- [ ] 電球の自動の要求（`triggerKind` 2、`only` なし）で `fix_all` を計算しない。明示の要求の結果は今と同じ。
- [x] 段 13 の計測を、直す前と後でこのチケットに記録している。
- [ ] T0・T1・T2（`node tests/lsp_sessions.mjs target/debug/tsuzuri`、`cd vsc && npm run test:unit && npm test`（`src/lsp.rs` を変えた後は、先に `npm run toolchain` で `vsc/toolchain/bin/tsuzuri` を作り直す。拡張のテストは同梱のコンパイラを使う）、`node scripts/check-docs.mjs <変更したページ>`）と、`ci:full` の `vscode.yml`・`windows.yml` が通る。

## 落とし穴

- `vscode.languages.getDiagnostics()` は、タスクのマーカー（拡張の外の owner）の削除を、同じ URI に拡張の診断が残っている間は反映しないことがある。
  VS Code 1.103.2 の `MainThreadDiagnostics` は、その URI のマーカーが 0 件になるか、外の owner のマーカーが残るときだけ拡張側へ送る（`workbench.desktop.main.js` の `if(n.length===0)t.push([s,[]]);else{const r=n.filter(o=>o?.origin!==this.e);r.length>0&&t.push([s,r])}`）。
  タスクのマーカーの検査は、`editor.action.marker.next` で UI のマーカーを巡るか、拡張の診断のない URI で行う。
- `workspace.openTextDocument` で開いた文書は、エディターを閉じても開いたままで、`didClose` が送られない。閉じる場面の検査は、Explorer と同じ `vscode.open` コマンドで開く。
- Run・Test・Debug は実行の前に `saveAll` する。その保存の watcher のイベントは、実行の結果を入れた後に届くことがある（macOS の FSEvents は遅れる）。
  watcher の変更の mtime は、その根で最後に始めた実行の開始と比べ、実行より前の保存のイベントで新しい結果を消さない（前の実行の開始と比べると、遅れて届いた `saveAll` の事象で新しい実行の結果を捨てる）。
- 実行の途中で根が変わったら、その実行の結果は入れない。入れた後に消すだけだと、結果を入れる処理が後から古い結果を入れ直す。
  根ごとに、仕様 3 の「変わった」が起きるたびに増やす番号を持ち、コンパイラを起動する直前に読んだ番号と結果を入れるときの番号が違えば入れない（エディターでの編集は mtime を変えないので、mtime だけで判定すると実行中の未保存の編集の後に古い結果が入る）。
- VS Code は macOS と Windows で、大文字・小文字だけが違う URI を 1 つにまとめる（`IUriIdentityService.asCanonicalUri`）。大文字・小文字を区別して数える driver は、残る診断を多く数えすぎる。Linux では別の URI です。
- 段 6 の集合は、読み込みの成功と失敗の両方の経路で更新する。`[]` は `closed` の抑止の対象外（今も空の一覧は閉じた文書へ送る。`src/lsp.rs:347`）。
- 段 9 の根の外の判定は、refresh した根のディレクトリ（`project_root` の値）で行う。依存の `package.id.root` で判定すると、根の中の依存まで外れる。
- 段 3 の重複の判定は、言語サーバーが遅れていると外れて一時的に 2 件並ぶ。段 2 で次の変更のときに消えるので、残り続けることはない。重複を除くために待つ処理は入れない。
- 段 11 の -32801 は、要求の文書に後の text sync が待ち行列にあるときだけ使う。通知は捨てず、`didOpen`・`didClose` は届いた順に処理する。`didChange` をまとめても、`document versions must increase` の検査は最後の版で行う。
- 段 11・12 の検査に、経過時間の閾値を置かない。応答の種類（-32800・-32801・結果）と、届いた診断の版で判定する。
- 原因 5 の 8.8〜20.4 秒は、refresh とコードアクションの時間が編集の間隔に近いときだけ起きる連鎖です。計測は同じプロジェクト・同じ打鍵の間隔で比べ、典型の値として書かない。

## 対象外

- どのクライアントがどの文書を受け持つかの見直し（入れ子のパッケージを覆う根のクライアント、このリポジトリを開いたときの `E1017`）。[G40](_completed/G40-fallback-root-client.md) で扱う。
- 4 MiB を超える `didChange` を受け付けて `E0003` を出すこと。
- [wasm] の検査のエラーを絶対パスで出すこと（`src/main.rs:4167-4175`）。
- 言語サーバーでのケーパビリティの検査（`E1036` を入力中に出す）。`packages.md` の「言語サーバーは検査しません」を変えることになる。
- Run・Build・Check を `CustomExecution` と疑似端末で動かし、`--json` の結果を拡張の集まりへ入れること（D3 の代案）。
- タスクの後に残る `a.o`。
- 1 回の refresh の費用を減らすこと（エディターの経路の std の全モジュールの検査、`unused_private` の std の走査）。[PB08](../_perfs/PB08-lsp-refresh-cost.md) で扱う。
- 解析を別のスレッドで動かし、古い解析を途中で捨てること。
- クイックフィックスの検証を `codeAction/resolve` へ遅らせること（修正案は検証したものだけを出す、という今の約束を変える）。
- エラーがある間にほかのファイルの警告が消えること、超線形の入力（関連する観測を参照）。

## 決定事項

- **D1: テストの発見は診断を出さない**（既定案）。理由: 発見はエディターのバッファではなくディスクを読み、言語サーバーは同じ前段の診断を入力のたびに出している。成功したときに消す処理を足すだけでは、エラーの間の重複と、ディスクの本文に基づく写しが残る。
  失う表示は、保存のたびの `E1036` だけで、これは明示したテストの実行・テストのデバッグ（発見の失敗として。仕様 2）、Debug Project、端末に出る。
- **D2: `tsuzuri-build` は、最後のテストの実行・テストのデバッグ・Debug Project の結果を、その根のプロジェクトが変わるまで持つ**（既定案）。言語サーバーと URI・コード・範囲の開始が同じものは除く。
  コードの範囲（`E0xxx`・`E1xxx`・`Wxxxx` は言語サーバーの担当）で除く案は採らない。言語サーバーの解析は CLI のすべての段を通るとは限らず、CLI だけが出す `E1xxx` を隠すおそれがある。
- **D3: 拡張のタスクから problem matcher を外す**（既定案）。理由: 拡張はほかの owner のマーカーを消せない。`applyTo: "closedDocuments"` は開いた文書にマーカーを作らないだけで、閉じた文書のマーカーは次のタスクまで残る。
  `CustomExecution` と疑似端末で `--json` を拡張の集まりへ流す案は、Run の標準入力（`IO.read_line`）を中継する端末を作り直すことになるので、このチケットでは採らない。
  `$tsuzuri` の定義は `package.json` に残す（利用者の `tasks.json` が名指ししている可能性がある）。
- **D4: `tsuzuri-build` の項目は根単位で消す**（既定案）。1 つのファイルの変更が、ほかのファイルの診断を変えるため。
- **D5: 言語サーバーは、根ごとに空でない診断を送ったパスとそのときの URI を覚え、送らなくなったパスと使わなくなった URI に `[]` を送る**（既定案）。パスだけを覚えると、消すときに作り直す URI が、空でない一覧を送った URI と違うことがある（4e の仕組み）。
  Copilot のレビュー（PR #119）で補った: 同じサーバーの別の根の記録にまだある URI には `[]` を送らない（下の「チケットから外れた判断」8）。
- **D6: 読み込みの失敗の間、ほかのファイルの前の解析の診断は消す**（既定案）。理由: 解析できない状態で前の結果を出し続けると、直したファイルのエラーが理由の表示なしに残る（4c）。
  失敗の理由は、そのファイルが閉じていても出す。前の解析の結果を薄く残す（タグを付ける）案は、LSP の診断に「古い」を表す標準の印がないので採らない。
- **D7: `didChangeWatchedFiles` は、変わった URI を正準化して入るクライアントの根と、この session で診断を送った根を解析し直す**（既定案）。どのクライアントの根にも入らない URI から、新しい根を作らない。
- **D8: 開いていないファイルの URI は、クライアントが `initialize` で送った根の表記で作る**（既定案）。どの根にも入らないパスは `file_uri` のまま。
- **D9: クライアントの根の外にある利用者のソースには診断を送らない**（既定案）。理由: 根の外のファイルの変更は、その根のクライアントの watcher に届かず、送った診断を直す手段がない。そのファイルは、開けばその根のクライアントが受け持つ。
- **D10: 後の text sync が届いている文書への要求は `ContentModified`（-32801）で答える**（既定案）。理由: その結果は古い本文に基づき、クライアントはどのみち捨てる。LSP 3.17 の `ContentModified` はこの用途のためにあり、VS Code は表示せずに読み捨てる。
- **D11: 自動の要求（`triggerKind` 2）で `only` がなければ `source.fixAll.tsuzuri` を計算しない**（既定案）。理由: VS Code 1.103.2 は自動の要求の結果から source の action を捨てるので、見える変化がない（`workbench.desktop.main.js` の `xes`）。
  `triggerKind` のない要求は明示（1）として扱い、今と同じく返す（`only` を送らないほかのエディターの明示の要求を変えない）。
- **D12: クイックフィックスの検証は残す**（既定案）。検証の費用は、取り消し（段 12）と `fix_all` の省略（D11）で、使われない計算だけを除く。検証を外すか遅らせる案は、言語リファレンスの約束を変えるので採らない。

## 実装と検証（Phase 1、2026-11-19）

Phase 1（段 1〜5）を実装した。Phase 2・3 は未着手なので、状態は `doing（Phase 1 done）` のままで、このファイルは `_features/` に置く。
着手時の HEAD は `4d612da4`（ブランチ `fix/g39-phase1-extension-diagnostics`）。コンパイラ・言語サーバー・`vsc/package.json` の `problemMatchers` の定義は変えていない。
拡張のテストは同梱のコンパイラ（`vsc/toolchain/bin/tsuzuri`。SHA-256 は `target/release/tsuzuri`（`af644794`）と同じ `48068cd7…`）で動かした。

### 実装

- 段 1（`vsc/src/testing.ts`）: `discover` は `publishDiagnostics` を呼ばない。コンパイラの失敗は stderr を持つ `DiscoveryError` として投げ、今と同じく `project.error` に出す。
- 段 2:
  - `vsc/src/workflow.ts`: `diagnosticsByRoot` を根ごとの記録 `RootResult`（結果の URI、結果が根の外を指すか、結果を出した実行の開始時刻、根の最後の変更の番号、根の外の最後の変更の番号、watcher が作成・変更を届けたファイルの最新の時刻（根の下と根の外の 2 つ）、根の下の削除の watcher）に置き換えた。
    `begin(root)` は、コンパイラを起動する直前の 2 つの番号と時刻を返す。`publishDiagnostics(root, text, start)` は、根の番号が変わったか、根の下にその実行の開始より新しい書き込みがあれば入れず、根の外を指す結果は根の外の番号と書き込みも比べる。入れるときは、その根の前の項目を URI ごとすべて消してから入れ（D4）、その実行の開始時刻を記録する。
  - 番号で数えるもの（時刻を持たない変化）: `onDidChangeTextDocument`（`contentChanges` が空でない `file` の文書。根の下はどの文書も、根の外は `.tz`・`.tt`・`.tc`・`.toml`・`Tsuzuri.lock`）、根ごとの `RelativePattern(root, '**')` の削除だけを見る watcher（`.` で始まる名前と、どの深さでも `target/tz/` の下を除く。`core.ts` の `affectsRoot`）、
    ワークスペース全体の `**` の削除だけを見る watcher（根の外の削除。ワークスペースのフォルダーから見て同じ名前を除く）、`onDidChangeWorkspaceFolders` の削除（その根の記録ごと消す）。
    根の下の変化はその根の番号を増やして項目を消す。根の外の変化は根の外の番号を増やし、根の外を指す結果だけを消す。
  - 時刻で数えるもの: ワークスペースの watcher `**/{*.tz,*.tt,*.tc,*.toml,Tsuzuri.lock}` の作成・変更（`core.ts` の `sourceFiles`）。ファイルの mtime と ctime の大きい方を、根の下か根の外の最新の書き込みとして記録し、出ている結果を出した実行の開始より新しければ、その結果を消す（根の外の書き込みは、根の外を指す結果だけ）。動いている実行は、結果を入れるときに自分の開始と比べる。
  - `vsc/src/testing.ts`: `run`・`debug` の発見は `discoverForRun` を通す。動いている発見があれば終わるのを待ち、`begin` してから発見を始め、`DiscoveryError` ならその stderr をその実行の結果として `publishDiagnostics` へ渡す。
    テストのコンパイル・テストのデバッグのビルド・Debug Project のビルド（`workflow.ts` の `debugConfiguration`）も、コンパイラを起動する直前に `begin` する。
  - `vsc/src/core.ts`: `inRoot`（Windows は大文字・小文字を区別しない）、`affectsRoot`、`projectFile`、`sourceFiles`。
- 段 3: `core.ts` の `sourcePosition`（BOM を除き、CRLF・CR・LF を改行として数え、UTF-16 の桁を返す。CRLF の LF を指す offset は行末）と `reportedByServer`（コードと範囲の開始が同じ）。
  `publishDiagnostics` は、入れる直前の `vscode.languages.getDiagnostics(uri)` の source `tsuzuri` と重なる記録を除く。
- 段 4: `workflow.ts` の `task` の problem matcher を `[]` にした。`package.json` の `$tsuzuri` は残した。
- 段 5: `vsc/README.md` の「コードを書く」「実行とビルド」「テスト」と、`vsc/CHANGELOG.md` の `## Unreleased`（公開のときに版の名前にする）。
- 統合テストの fixture（`vsc/scripts/test-extension.mjs`）に、3 つ目のワークスペースフォルダー `packages`（`capabilities`: `[capabilities] allow = []` で `IO` を使う。`app`: path 依存 `../lib` と、ソースでない `notes.txt`。`lib`: `Calc.tz`・`Shapes.tz`・`Extra/Note.tz`）を足した。

### テスト（各段で先に書き、直す前に落ちることを確かめた）

記録は `target/scratch/g39-phase1/`（コミットしない）にある。

| 段 | テスト | 直す前 |
| --- | --- | --- |
| 1 | `extension.test.ts`: `Main.tz` にエラーを保存して発見の失敗（`roots.get(root).error`）を待ち、直して保存して `error` が `undefined` に戻るのを待つと、source `Tsuzuri` の診断がない | HEAD で `['E1002']` が残って落ちた（`step1-before.out`） |
| 2 | `extension.test.ts`: `capabilities` で未保存の編集のあとテストを実行すると、直後に source `Tsuzuri` の `E1036` がある。saveAll の watcher の事象（最長 3 秒）と 0.5 秒の後も残る。本文の編集、フォルダー `Extra` の削除、`Tsuzuri.toml` のディスクでの書き換えのそれぞれで消える | 段 1 だけの状態で `0 !== 1`（実行が発見の失敗を出さない）で落ちた（`step2-before.out`） |
| 2 | `extension.test.ts`: `app` のテストの実行で、`Shapes.tz` の型が原因の `lib/Calc.tz` の `E1003` が出て、`Shapes.tz` の書き換え（結果の URI でないファイル）で消える。`Calc.tz` の `W1001` が出て、実行の途中（実行の発見が済んだ後）に `Shapes.tz` を書き換えると入らない。`Calc.tz` の `E1003` は、エディターで開いて直して保存すると消える | 段 1 だけの状態では上の検査で止まる。途中の変更の検査は、`outside` の比較を外した変異で `W1001` が残って落ちることを確かめた（`step2-mutant-no-inflight-check.out`） |
| 2 | `extension.test.ts`（`supportsDebug()` の分岐）: Debug Project の直後に `E1036` があり、本文の編集で消える | 同上 |
| 2 | `core.test.ts`: `inRoot`・`affectsRoot`・`projectFile` | 関数がなく型検査で落ちた（`step2-unit-before.out`） |
| 3 | `core.test.ts`: `sourcePosition`（BOM・CRLF・CR・補助平面・CJK・文字の途中・範囲外。期待値は手で数えた）、`reportedByServer` | 関数がなく型検査で落ちた（`step3-unit-before.out`） |
| 3 | `extension.test.ts`: source `tsuzuri` の `W1001` を待ってテストを実行すると、直後の `W1001` は `tsuzuri` の 1 件だけで、`Tsuzuri` は `E2006` だけ | 段 2 までの状態で `['Tsuzuri', 'tsuzuri']` で落ちた（`step3-before.out`） |
| 4 | `extension.test.ts`: 警告の状態で check と run のタスクを流し、`editor.action.marker.next` でマーカーがあることを確かめ、直して保存して言語サーバーの警告が消えた後にマーカーがない | 段 3 までの状態で `true !== false`（タスクのマーカーが残る）で落ちた（`step4-before.out`） |
| 2（レビュー） | `extension.test.ts`: `app` の実行の発見が済んだ後（実行は同じタスクで番号を取ってコンパイラを起動している）に、根の外の `other/Main.tz` をエディターで編集すると、根の外を指す `W1001` が入らない。根の下の `notes.txt` をエディターで編集しても入らない | `outside` の比較を外した変異と `changed` の比較を外した変異で、それぞれ `W1001` が残って落ちた（`fix/mutant-no-outside-check.out`・`fix/mutant-no-changed-check.out`） |
| 2（レビュー） | `extension.test.ts`: `app/Main.tz` の失敗するテストで `E2006` を出し、ディスクで直した直後に、すべてのテストを除いた実行（発見の後で止まり、結果を出さない）を始めても、`E2006` が消える | レビューの前の `workflow.ts`（根の最後の実行の開始と比べる）で、消えずに時間切れで落ちた（`fix/mutant-old-workflow.out`） |
| 2（レビュー） | `extension.test.ts`: `capabilities/Fresh.tz` の作成で `E1036` が消える。`lib/Extra` の削除（根の外のフォルダー）で `lib/Calc.tz` の `E1003` が消える。最後に、ワークスペースから `packages` を外すと `E1036` が消える | 作成の購読を外した変異、根の外の削除の watcher を外した変異、フォルダーの削除の処理を外した変異で、それぞれ時間切れで落ちた（`fix/mutant-no-create.out`・`fix/mutant-no-removals.out`・`fix/mutant-no-folder-removal.out`） |
| 2（レビュー） | `core.test.ts`: `affectsRoot` の入れ子の `target/tz/`、`projectFile` の `Tsuzuri.lock` | — |

既存の build・run・check のタスクの終了コードの検査は、同じ内容を `runTask` にまとめただけで、期待値は変えていない。

### 確認（macOS 27.2、Apple Silicon、VS Code 1.103.2（`vsc/.vscode-test`）、Node 24.22.2）

- `cd vsc && npm run test:unit`: 19 passed、0 failed（前は 16 件。3 件を足した）。
- `npm run check-types`、`npm run lint`: エラーなし。
- `node ../scripts/slot.mjs -- npm test`（`ELECTRON_RUN_AS_NODE` と `VSCODE_*` を外す）: 12 の段の `VS Code: …` と `All Tsuzuri VS Code integration checks passed.`（約 60 秒。続けて 2 回繰り返しても通った。`final-vsc.out`・`final-vsc-repeat2.out`・`final-vsc-repeat3.out`）。レビューの後の結果は下の「レビューの 1 回目」。
- `node tests/platforms.mjs`: 成功。`git diff --check`: 差分なし。`node scripts/check-links.mjs`: 0 broken。
- Rust のテストと `tests/lsp_sessions.mjs` は、コンパイラと言語サーバーを変えていないので動かしていない。`ci:full` の `vscode.yml`（5 環境）は push していないので未実行（PR には `ci:full` のラベルが要る）。
- 受け入れ条件のうち Phase 1 の 4 つ（保存したエラーの写し、テストの実行・Debug Project の結果、重複、タスクのマーカー）を上のテストで確かめた。ただし「実行の途中で根が変わったら、その結果は入らない」は、最初の版では根の外の変化だけを検査していた。レビューの後は、根の下と根の外のエディターでの編集の両方を検査する。実行の途中のディスクの書き込みは、実行の後に届いた事象と同じ時刻の比較で判定し、その比較は実行の後の書き込みの検査が確かめる。自動保存は、保存と同じ経路なので統合テストに足していない。端末の `path:line:column` は CLI が絶対パスで出し、VS Code の端末のリンクの検出で開くが、クリックは自動で確かめていない。

### チケットから外れた判断

1. **変化を 2 種類に分けて数える。時刻を持たない変化は番号で、watcher の作成・変更はファイルの時刻で、それぞれの実行の開始と比べる。**
   仕様 3 は「根の外の変化の最後の時刻を 1 つ記録する」と書き、設計と落とし穴は、watcher の作成・削除と、その根で最後に始めた実行の開始より新しい mtime の変更で番号を増やすと書く。
   実装は、エディターでの編集・削除（根の下は根ごとの watcher、根の外はワークスペースの watcher）・ワークスペースのフォルダーの削除を、根の番号と根の外の番号で数える。この比較はミリ秒の時刻を持たない。根の番号も同じ通し番号で数えるので、ワークスペースから外して足し直した根が、古い実行の番号と一致しない。
   watcher の作成・変更は番号を増やさず、ファイルの mtime と ctime の大きい方を、根の下と根の外のそれぞれの最新の書き込みとして記録する。動いている実行は、結果を入れるときに自分の開始と比べる。出ている結果は、それを出した実行の開始と比べて消す（レビュー R1）。
   根で最後に始めた実行の開始と比べる方法は、後の実行が結果を出さずに止まると、その開始より前の書き込みを数えず、古い結果が残った。止まるのは、選んだテストが 0 件、テストのデバッグで 2 件以上を選んだ、CodeLLDB を入れられない、取り消しのとき。同じ根の 2 つの実行が重なって古い方が後に終わったときも残った（`target/scratch/g39-phase1/review-races/masked.out`・`overlap.out`）。
   落とし穴の意図は変わらない。実行の前の saveAll の事象が実行の開始の後に届いても、その書き込みは実行の開始より前なので、その実行の結果を捨てない。根の外の作成・変更の事象も、仕様 3 の「届けていれば入れない」ではなく、同じくファイルの時刻で判定する。実行の前の書き込みの事象が遅れて届いても、その実行の結果を捨てないためです。
   残る境界: 実行の開始と同じミリ秒の書き込みは、実行の前の書き込みとして扱う。Linux と Windows のファイルの時刻の粗さ（数ミリ秒から 1 tick）の分も同じです。コンパイラはその後に起動してソースを読むので、その結果は書き込みの後の本文に基づく見込みが高い。
2. **watcher の作成の事象も、ファイルの時刻で判定する。** 設計は作成・削除をつねに変更に数える。VS Code 1.103.2 の macOS の watcher は、セッションで最初の数回の保存を報告せず、後の保存を作成として届けることがある
   （`target/scratch/g39-phase1/watch-probe/gaps.out`: 起動の前に作ったファイルを保存すると、1・2 回目は事象なし、3 回目が `create`、以後は `change`）。作成をつねに数えると、実行の saveAll の遅れた作成の事象が、その実行の結果を消す。
   作成と変更はどちらも、mtime と ctime の大きい方を記録する（ctime は移動で新しくなるので、mtime を保つ移動も数える）。削除は今までどおりつねに数える。
3. **明示した実行の発見は、動いている発見の終わりを待ってから始める。** 設計の「発見を始める直前の変更の番号」を守るため。saveAll の前に始まった裏の発見に相乗りすると、saveAll の前に読んだ本文の失敗を、saveAll の後の番号で入れてしまう（saveAll の書き込みはその実行の開始より前なので、時刻の比較でも変化に数えない）。
4. **統合テストの fixture に 3 つ目のワークスペースフォルダー `packages` を足した。** 既存のプロジェクトに `Tsuzuri.toml` を足すと既定の名前空間が変わり、既存の検査に影響するため。
5. **saveAll の後で結果が消えないことの検査は、watcher の事象を最長 3 秒待つだけで、事象が届かなくても落とさない。** 上の 2 のとおり macOS では事象が届かないことがあるため。事象が届く環境（Linux・Windows の CI）で意味を持つ。
6. **`Tsuzuri.lock` も変化に数える（レビュー R2）。** 仕様 3 の `.tz`・`.tt`・`.tc`・`.toml` に足した。依存の取得と検証の誤り（`E2007` の `Tsuzuri.lock is missing` など）は実行の結果にしか出ず、端末の `tsuzuri fetch` で lock を作っても次の変更か実行まで残った。
   lock を書くのは `tsuzuri fetch` だけで（`src/fetch.rs` の `write_lock`）、拡張の実行（`test`・`test --list`・Debug Project のビルド）は書かないことを確かめた。実行が自分の書き込みでその結果を捨てることはない。
7. **根の外の削除も watcher で数える（レビュー R3）。** フォルダーごとの削除は VS Code がフォルダーの 1 件の事象にまとめるので、根の外の依存のフォルダーを消しても、ソースの watcher（`**/*.{tz,tt,tc,toml}`）に届かず、依存を指す結果が残った。設計は根の下にだけ削除の watcher を置いていた。
   ワークスペース全体の `**` の削除だけを見る watcher を足し、根の外の削除を根の外の番号に数える。除く名前は根の下と同じで、ワークスペースのフォルダーから見る。ソースの watcher の削除の購読は、根ごとの watcher とこの watcher に任せて外した。
   `affectsRoot` は、どの深さの `target/tz/` の下も除くようにした（前は根の直下だけ）。入れ子のパッケージや根の外の依存の知識グラフを、その実行が書き換えるため。

### 残る問題

- 根の外からフォルダーごと根の下へ移動すると、VS Code はフォルダーの作成の 1 件にまとめるので、ソースの watcher にも削除の watcher にも届かず、結果が残る（`vsc/README.md`・`vsc/CHANGELOG.md` にも、数えないと書いた）。設計どおり削除だけを見ているため。
  フォルダーの作成も数えるには、最初の実行が根に作る `target/` を除くかを、チケットの持ち主が決める必要がある。除かないと、その実行の結果を捨てる。モジュールの発見は `target/` を読み、飛ばすのは知識グラフの印のあるディレクトリだけです（`src/driver.rs` の `collect_sources`）。
  最初の版はこの理由に「Debug Project のビルドが根に作る `a.o`」も挙げたが、誤りだった。拡張と同じ引数の `test --list`・`test`・テストのデバッグ・Debug Project のビルド・`build`・`check` は、根の `.tsuzuri/` と `target/` の外に何も作らない（レビューの `target/scratch/g39-phase1/review-spec/watch.mjs` と、手元で同梱のコンパイラを動かして確かめた）。`a.o` は「関連する観測」のとおり、タスクの後に見たもの。
- `[native]` のリンクの入力（`.o`・`.a` など）の作成・変更は、変化に数えない（レビュー R2）。入力がないときの `E2001`（`cannot read link input`）などが、入力を作った後も次の変更か実行まで残る（`target/scratch/g39-phase1/review-races/native.out`）。
  入力はマニフェストが名指しする任意のパスで、根の外にもありうる。拡張子で watcher を置く案は、ツールチェーンやタスクが根に書くもの（上の `a.o`）でその実行の結果を捨てるおそれがある。仕様 3 にない入力なので、チケットの持ち主の判断を待つ。それまでは `vsc/README.md`・`vsc/CHANGELOG.md` で、実行し直すように書いた。
- 根の下の削除は、名前によらず変化に数える（`.` で始まる名前と `target/tz/` の下を除く。設計のとおり）。ソースでもフォルダーでもないファイル（例えば `notes.txt`）を消しても結果が消える。実行の途中にテストや Debug Project のプログラムが根の下のファイルを消すと、その実行の結果が入らない（Testing ビューの成否は出る）。
  古い結果が残る向きではない。名前で絞る案（レビュー R4）を採らなかった理由は「レビューの 1 回目」に書いた。
- 重複の除外は、入れる時点の言語サーバーの診断で判定するので、言語サーバーが遅れていると一時的に 2 件並ぶ（落とし穴のとおり、次の変更で消える）。
- 2 つの根が同じ URI に結果を出すと（`app` と `lib` が `lib/Calc.tz` に）、後の方が前の方を置き換え、どちらかの根の変化でその URI が消える。古い結果が残る方向ではない（前の `diagnosticsByRoot` と同じ）。
- 根の外の watcher の事象は、ワークスペースのフォルダーの中だけに届く。ワークスペースの外の path 依存の変化は、エディターでの編集だけで数える（設計のとおり）。

### レビューの 1 回目（2026-11-20）

独立した 3 人のレビュー（仕様、競合、テスト）の指摘 13 件を、コードと照らして確かめた。記録と harness は `target/scratch/g39-phase1/`（`review-spec/`・`review-races/`・`review-tests/`、直した後の確認は `fix/`）にある。

| 指摘 | 結果 | 内容 |
| --- | --- | --- |
| G39P1-SPEC-1・T1 | 直した | 根の下で実行の途中に変わる場面を検査していなかった。`workflow.ts` の `state.changed !== start.changed` を外しても、すべてのテストが通った。根の下の `notes.txt` をエディターで編集する検査を足した（上のテストの表）。エディターの編集は watcher もファイルの時刻も通らないので、決定的に落ちる。「確認」の受け入れ条件の文も直した |
| G39P1-SPEC-2 | 直した | 根の外の結果の検査に、言語サーバーも出す `W1001` を使っている。写しが出るのは lib を受け持つクライアントがないからなので、その前提（`lib/Calc.tz` に source `tsuzuri` の診断がない）を、写しを数える前に毎回検査し、理由をコメントに書いた |
| G39P1-SPEC-3 | 直した（文書） | 外からフォルダーごと移すと消えないのに、README・CHANGELOG は移動を無条件に数えると書いていた。消えるのは削除と、ほかの場所への移動・名前の変更だけで、外から移したフォルダーは数えないと書き直した。残る問題の理由（`a.o`）の誤りを直した。フォルダーの作成を数えるかは `target/` の判断が要るので、残る問題に残した |
| R1 | 直した | 書き込みの事象を、根で最後に始めた実行の開始と比べていたので、後の実行が結果を出さずに止まるか、重なった古い実行が後に終わると、古い結果が残った。それぞれの実行の開始と比べるようにした（外れた判断 1）。結果を出さずに止まる実行の検査を足した |
| R2 | 一部を直した | `Tsuzuri.lock` を数える（外れた判断 6）。`[native]` のリンクの入力は仕様 3 にない入力で、置き方にチケットの持ち主の判断が要るので、残る問題に書いた。`Tsuzuri.lock` の統合テストは足していない（作成・変更は同じ watcher と時刻の比較を通り、その経路は `Fresh.tz` と `Tsuzuri.toml` の検査が確かめる。名前の照合は `projectFile` の単体テスト） |
| R3 | 直した | 根の外の依存のフォルダーを消しても、依存を指す結果が残った。ワークスペース全体の削除の watcher を足した（外れた判断 7）。`lib/Extra` を消す検査を足した |
| R4 | 採らない | 根の下の、ソースでもフォルダーでもないファイルの削除でも結果が消える。設計が選んだ watcher のとおりで、消すのが早すぎるだけで古い結果は残らない。提案の絞り込み（拡張子のない名前と、プロジェクトのファイル・入力だけを数える）は、モジュールの発見が `.` で始まらないすべてのディレクトリを読むので、名前に `.` を含むフォルダー（`v1.2` など）の削除を数えず、そこにあったソースの古い結果を残しうる。G39 は古い結果を残さない方を優先する。残る問題に書いた |
| T2 | 直した | 根の外の途中の変更をディスクに書いていたので、書き込みが実行の開始と同じミリ秒に入るか、watcher の事象がコンパイルの終わりより遅れると、誤って落ちえた。エディターでの `other/Main.tz` の編集に替えた。`lib` のファイルは開かない（lib のクライアントが `W1001` を除き、検査が空振りする） |
| T3 | 直した | README・CHANGELOG に書いた変化のうち、ソースの作成とワークスペースのフォルダーの削除を検査していなかった。`capabilities/Fresh.tz` の作成と、`packages` を外す検査を足した。`packages` は 3 つ目のフォルダーなので、外しても拡張ホストは再起動しない。言語サーバーが再起動するので最後に置いた（Windows ARM64 でデバッグの節を飛ばすときも、戻る前に行う） |
| T4 | 直した | 外れた判断 1 が、根の外の watcher の事象もファイルの時刻で判定していることと、そのミリ秒の境界を書いていなかった。1 と 2 を書き直した |
| D1 | 直した | 言語サーバーの診断は Phase 2・3 の前なので、まだ消えない場面と遅れが残る。README の「すぐに」を外し、CHANGELOG の見出しを拡張の `Tsuzuri` の写しとタスクのマーカーに絞った |
| D2 | 直した | README・CHANGELOG のコードの分類を直した（リンクの失敗は `E2002`、`E2006` も `E2xxx` の 1 つ） |

レビューの後の確認（macOS 27.2、VS Code 1.103.2、Node 24.22.2）:

- `cd vsc && npm run test:unit`: 19 passed、0 failed。`npm run check-types`、`npm run lint`: エラーなし。
- `node ../scripts/slot.mjs -- npm test`（環境変数を外す）: 13 の段の `VS Code: …` と `All Tsuzuri VS Code integration checks passed.`（約 70〜100 秒。`fix/run1.out` と、文書を直した後に続けて 2 回の `fix/final-1.out`・`fix/final-2.out`）。
- 変異（`fix/mutants-summary.out`）: レビューの前の `workflow.ts`、`changed` の比較なし、`outside` の比較なし、根の外の削除の watcher なし、作成の購読なし、フォルダーの削除の処理なし。どれも、その場面の検査で落ちた（順に、結果を出さずに止まる実行の後の `E2006` の時間切れ、`notes.txt` の検査の `W1001`、`other/Main.tz` の検査の `W1001`、`lib/Extra` の削除の時間切れ、`Fresh.tz` の作成の時間切れ、フォルダーの削除の時間切れ）。
- レビューの harness（`review-races/runner.mjs`）で、R1（`masked`・`overlap`）と R3（`depfolder`）が直ったことを確かめた（`fix/harness-*.out`: `masked` の M3、`overlap` の O2、`depfolder` の D3 がどれも空）。`native` は、リンクの入力を作っても `E2001` が残り（N3。残る問題）、`notes.txt` の削除で結果が消える（N6。R4 を採らない）。
- `node tests/platforms.mjs`: 成功。`git diff --check`: 差分なし。`node scripts/check-links.mjs`: 0 broken。

### Copilot のレビュー（PR #117、2026-11-20）

| 指摘 | 結果 |
| --- | --- |
| 明示した実行の発見の間に、未保存のバッファで直すと、`run` が発見の後に取り直した開始で、保存された古い本文の `E2006` を出しうる。テストのデバッグも同じ（`ensureDebugger` の待ちの間の編集を含む） | 直した。`discoverForRun` は発見の前に取った開始を返し、`run`・`debug` はそれをコンパイルと結果の表示まで使う（取り直さない）。発見や CodeLLDB の導入の間の編集は、その実行の結果を古いものにする |
| ワークスペースのフォルダーの外の根（`tsuzuri.projectPath`、デバッグ構成の絶対パスの `project`）では、ソースの作成・書き込みを見る watcher がない（ワークスペースの watcher はフォルダーの中だけ） | 直した。根ごとの watcher（`RelativePattern(root, '**')`）が、削除に加えて、根の下のソースとマニフェスト（`projectFile`）の作成・書き込みを、ワークスペースの watcher と同じくファイルの時刻で扱う。ワークスペースの中の根では 2 つの watcher が同じ事象を受けるが、時刻の最大を取るだけなので結果は変わらない |
| 2 回目の概要（スレッドなし）: 明示した実行が、走っている裏の発見を待ってから開始を取るので、その待ちの間の未保存の修正が数えられない | 直した。開始を、走っている発見を待つ前に取る（`saveAll` は済んでいるので、ファイルの時刻の比べ方は変わらない） |
| 3 回目の概要（スレッドなし）: 明示した実行・テストのデバッグが、`saveAll` の後の `refreshRoots()` や前の根の実行を待ってから開始を取るので、その間の未保存の修正が数えられない。Debug Project は `saveAll` の後に `projectFor()` を待つ | 直した。開始を変更の番号の通し番号（`epoch`）にし、`saveAll` の直後に 1 回だけ取って、すべての根（設定の途中で初めて見つかった根を含む）で使う。根の変更の番号が `epoch` より後なら古い結果とみなす。新しく作った根の状態は番号 0 で始め、ワークスペースのフォルダーの削除はその番号を記録して、削除の前に始まった実行の結果を出さない。Debug Project は根を `saveAll` の前に求める |
| 4 回目: `refreshRoots()` が開始の後に見つけた根では、`track()` までの編集が記録されない（変更の扱いは登録済みの根だけを更新する）ので、新しい根を番号 0 で始めると古い結果が出る | 直した。後から追う根は、その時点の最後の変更の番号で始める（開始の後にどこかで何かが変わっていれば、その実行の結果を出さない。関係のない変更は結果を早く消すだけ）。テストのために拡張の API に `workflow`（`begin`・`track`・`publishDiagnostics`）を足し、`begin` と `track` の間の編集で結果が出ず、編集がなければ出る統合テストを足した。新しい根を番号 0 で始める変異では `1 !== 0` で落ちる |

確認: `npm run check-types`・`lint`・`test:unit`（20 件成功）・`npm test`（14 の `VS Code:` 行と `All Tsuzuri VS Code integration checks passed.`）。どれも統合テストでは時刻の窓を決めて再現できないので、テストは足していない（コードの読みと既存の検査で確かめた）。

### CI で見つけたこと（PR #117 の `ci:full`、2026-11-20）

- Windows x64 と Windows Arm64 の `VS Code integration` だけが、テストの実行の直後の `E1036` の検査（`0 !== 1`）で落ちた。macOS・Linux は通った。
- 原因: コンパイラの `--json` の `path` は正準化した絶対パスで（macOS ではシンボリックリンクを解いた `/private/tmp/...`）、Windows では `fs::canonicalize` の verbatim パス `\\?\D:\...\Main.tz` になる（`.github/instructions/ci.instructions.md` の注意どおり）。拡張はそれをそのまま `path.resolve` して URI にしていたので、診断が `Main.tz` とは別の URI に付き、根の外の結果としても扱われた。G39 の前から、テストの実行・Debug Project の写しは Windows で別の URI に付いていたことになる。
- 直したこと: `vsc/src/core.ts` に `plainPath`（`\\?\C:\...` を `C:\...`、`\\?\UNC\server\share` を `\\server\share` にする）と `rootSpelling`（根の実のパスの下のパスを、VS Code が開いた根の綴りの下へ移す。シンボリックリンクを通した根のため）を足し、実行の結果の診断（`publishDiagnostics`）とテストの発見の項目の URI の両方で使う。単体テスト `compiler paths are spelled as VS Code names the project: no verbatim prefix, under the root as opened` を足した（20 件成功）。コンパイラの出力は変えていない。

### 更新した文書と変更したファイル

- 言語リファレンスは変えていない（Phase 1 は拡張だけを変える。チケットの「ドキュメント」も Phase 1 は `vsc/README.md` と `vsc/CHANGELOG.md`）。
- `vsc/src/workflow.ts`、`vsc/src/testing.ts`、`vsc/src/core.ts`、`vsc/src/test/extension.test.ts`、`vsc/src/test/core.test.ts`、`vsc/scripts/test-extension.mjs`、`vsc/README.md`、`vsc/CHANGELOG.md`、`_features/G39-stale-editor-diagnostics.md`、`_features/README.md`。

## 実装と検証（Phase 2・3、2026-11-20）

Phase 2（段 6〜10）と Phase 3（段 11・12・14）を実装した。段 13（計測）は、ほかの作業のない機械で別に取った（下の「段 13」）。
着手時の HEAD は `295ffcb4`（`main`、拡張 0.1.8 の公開の後。ブランチ `fix/g39-phase2-3`）。PB08 が同時に `src/lsp.rs` を変えるので、送った URI の記録・待ち行列・取り消しの確認は、それぞれ別の関数と型に分けた（下の「PB08 との合流」）。

### 実装

- 段 6（`src/lsp.rs`）: `Session::published`（根 → 空でない一覧を送ったパスと、そのときの URI）を足した。`Session::refresh` は送った先を `Sent` に集め、`Session::retire` が、前の記録にあって今回どの一覧も送らなかった URI に `[]` を送ってから記録を置き換える（同じパスを別の URI で送ったときの前の URI も、こうして消える）。読み込みの成功と失敗の両方の経路で呼ぶ。`publish` は送った URI を返し、閉じた文書への解析の一覧を捨てたときは `None` を返す（記録に入れない）。
  `refresh` は、パスではなく根のディレクトリを受け取るようにした（`pending` も根 → 期限）。記録の根や `didChangeWatchedFiles` の根を、ファイルのパスを介さずに解析し直すため。
- 段 7: 読み込みの失敗は `Session::load_error` で、`SourceError` の span を、開いているバッファか `driver::read_source_text` で読んだ本文の `PositionMapper` で範囲にする（読めないか span がなければ 0:0）。`publish(..., load_error = true)` は閉じた文書にも送る。ほかのファイルの前の診断は段 6 の記録で消える（D6）。
- 段 8: `workspace/didChangeWatchedFiles` は `Session::changed_roots` で、各 URI のパスを存在する最も近い祖先のディレクトリで正準化し（`canonical_existing`。`document_path` と同じく親から始めるので、パス自身がシンボリックリンクでもたどらない。消したファイルやフォルダーは残っている親の下に置く）、それが入るクライアントの根、開いているバッファの根、`published` の根を `schedule_root` する。どのクライアントの根にも入らないパスからは根を作らない（D7）。
- 段 9: `Session::client_roots`（`initialize` の `rootUri`・`workspaceFolders` のパスと URI と、そのときに正準化した根。そのときなかった根は使うときに正準化する。refresh が送るファイルごとに根を正準化し直さないため。Copilot の 2 回目のレビューで、refresh のたびに正準化し直すようにした（逸脱 12））と `uri_under` を足し、`Session::uri` を、バッファ → `closed` → クライアントの根の表記 → `file_uri` の順にした。診断・定義・参照・リネーム・コードアクションの URI はすべてこれを通る（D8）。`refresh` は、根のディレクトリの外の利用者のソースに解析の一覧を送らない（D9。読み込みのエラーは例外）。
  拡張（`vsc/src/extension.ts`）: 言語クライアントの `synchronize.fileEvents` に、根の `**` の削除だけを見る watcher を足した。middleware の `didChangeWatchedFile` は、`.tz`・`.tt`・`.tc`・`.toml` の事象（ソースの watcher が前から送っていたもの）をそのまま送り、それ以外のパス（`**` の削除の watcher だけが送るもの）を `affectsRoot`（Phase 1 と同じ。`.` で始まる名前と `target/tz/` の下を除く）に通す（`notifiesServer`。下のレビューの指摘 5）。
- 段 11（`src/lsp.rs` の `serve`）: 容量 8 の `sync_channel` をやめ、受信のスレッド（`read_input`）は、読んで主ループがまだ処理していない本文の合計（`Inbox`）が `MAX_QUEUED_BYTES`（64 MiB、`MAX_MESSAGE` の 4 倍）を超える間だけ `Condvar` で待つ。`$/cancelRequest` の ID は読んだときに `Inbox` の集合へ入れる（Copilot の 3 回目のレビューで、集合を ID ごとの件数にした。逸脱 13）。主ループは各メッセージの前にチャンネルを `try_recv` で空にして `Queue` へ移す。
  `Queue` は、待っている要求の ID と、文書ごとの後の `didChange`・`didClose` の件数を持つ。取り消された要求は計算せずに -32800、後に同じ文書の `didChange`・`didClose` がある要求は -32801（`content modified`）で答える。`didChange` を飛ばすのは、後に同じ文書の `didChange` があり、その間の要求がどれも計算されない（取り消されたか、自分の文書が後で変わる）ときだけだった（`Queue::superseded`。Copilot の 2 回目のレビューで、飛ばさずにすべて反映して検査するようにした。逸脱 10）。答えた ID は `Queue::answer` が集合から消す。
- 段 12: `handle` が取り消しを確かめる関数を受け取り、`code_actions` から `quick_fixes`（各修正案の `fix_edit` の前）と `fix_all`（読み込みの前）へ渡す。`src/fix.rs` の `run` は第 4 引数の `cancelled` を各ラウンドの解析の前に呼び、取り消されたらすべての本文を戻して `Stop::Cancelled` の報告を返す（CLI の `fix`・`tz_fix_plan`・`canonical_round` は取り消さない関数を渡す）。`context.triggerKind` が 2 で `only` がない要求には `fix_all` を呼ばない（D11）。
- 段 10・14（文書）: 下の「更新した文書」。

### テスト（各段で先に書き、直す前に落ちることを確かめた）

記録は `target/scratch/g39/`（このワークツリー。コミットしない）にある。`tests/lsp.rs` には、`serve` を別のスレッドで動かし、1 通ずつ送って出力を順に読む `live::Server` と、親の一時ディレクトリに根と依存を置く `live::Fixture` を足した（`Server::settle` は `workspace/symbol` で予定の根の解析を待つ）。

| 段 | テスト | 直す前 |
| --- | --- | --- |
| 6 | `files_that_leave_the_project_get_an_empty_list`: 開いていない `Helper.tz`（削除）、`Old.tz`（`New.tz` へ名前の変更）、`Sub/Inner.tz`（フォルダーごと削除）の古い URI に `[]` が届き、`New.tz` に `E1003` が届く | `Helper.tz` に `E1003` が残って落ちた（`phase2-before.out`） |
| 6 | `a_fixed_manifest_and_a_dropped_dependency_get_an_empty_list`: 壊して直した `Tsuzuri.toml` と、項目から外して消した根の中の依存 `lib/Calc.tz` に `[]` が届く | `Tsuzuri.toml` の `E0002` が残って落ちた |
| 7 | `a_load_error_clears_the_other_files_and_reaches_closed_documents`: マニフェストが壊れている間に直した `Main.tz` に `[]`、範囲が `Project::load` の span（CLI の `--json` と同じ）、開いて閉じた `Main.old.tz` に `E1011` | `Main.tz` の `E1003` が残って落ちた。`E1011` の部分は、閉じた文書の抑止を読み込みのエラーにも掛ける変異で `[] != ["E1011"]` で落ちた（`mutant-closed.out`） |
| 8 | `disk_changes_are_analyzed_without_open_documents`: 根の文書を閉じた後、ディスクで直した `Helper.tz` に `[]` が届く | 落ちた（`phase2-before-2.out`）。最初の版は、`didClose` 自身が根を解析し直すので HEAD でも通った。閉じた後の解析を待ってからディスクを変えるように直した |
| 8・9 | `a_linked_root_keeps_the_clients_spelling`（Unix）: シンボリックリンクの根で、開いていない `Helper.tz` の診断がリンクの表記の URI に届き、正準化した URI には届かない。`Sub/Util.tz` の変更の通知の後、`workspace/symbol` の `util` がちょうど 1 件（`link/Sub/Util.tz`）で、`Sub` やリンクの表記の根を別に解析していない（D9 は誤って解析した根の診断も隠すので、診断だけでは見分けられない。レビューの指摘 2 で足した） | リンクの表記の URI に何も届かず落ちた。`workspace/symbol` の検査は、`uri_path` の値をそのまま `project_root` に渡す変異で 2 件になって落ちる |
| 9 | `dependencies_outside_the_root_get_only_load_errors`: 根の外の path 依存 `../lib/Calc.tz` に解析の診断が届かず、`../lib/Tsuzuri.toml` の読み込みのエラーは届く | `lib/Calc.tz` に `E1003` が届いて落ちた |
| 6 | `tests/lsp_sessions.mjs`: 開いていない `Unused.tz` を消して `didChangeWatchedFiles` を送ると、その URI に `[]` が届く | 空の一覧を待って時間切れで落ちた（`sessions-before.out`） |
| 6・9 | `vsc/src/test/extension.test.ts`: 開いていない `Stale.tz` の `WorkspaceEdit.renameFile`・`workspace.fs.delete`、フォルダー `Stale` の名前の変更と削除で、古い URI の source `tsuzuri` の診断が消える | 同梱のコンパイラ（0.1.8）で `Timed out: diagnostics of the old name of a renamed file`（`vsc-base-compiler.out`）。拡張の `**` の削除の watcher を外す変異では `Timed out: diagnostics of a file in a renamed folder`（`vsc-mutant-no-folder-watcher.out`） |
| 11 | `a_request_cancelled_behind_many_messages_is_not_computed`: refresh で忙しいサーバーに `codeAction`、通知 10 個、その取り消しを送ると -32800 | 結果が返って落ちた（`phase3-before-final.out`） |
| 11 | `cancellations_of_unknown_ids_leave_room_for_later_ones`: 存在しない ID の取り消し 1100 個の後でも、`didChange` の直後の `hover` の取り消しが効く | 結果が返って落ちた |
| 11 | `a_request_sees_the_changes_that_arrived_before_it`: `didChange(A, 2)`・`hover(B)`・`didChange(A, 3)` の `hover(B)` が A の版 2 の型を答える | 通った（チケットのとおり、要求をまたいでまとめる誤りを見つけるための検査） |
| 11 | `a_request_for_a_replaced_text_gets_content_modified`: 別の文書の `hover` で忙しくしたサーバーに `didChange`（版 2）・`hover`・`didChange`（版 3）・`documentSymbol` を送ると、`hover` は -32801、`documentSymbol` は結果、版 2 の診断は届かず版 3 は 1 回だけ | `hover` が結果を返して落ちた |
| 12 | `automatic_code_action_requests_skip_fix_all`: `triggerKind` 2 で `only` なしは `quickfix` だけ、1 は `quickfix` と `source.fixAll.tsuzuri`、2 で `only: ["source.fixAll"]` は `source.fixAll.tsuzuri` | `triggerKind` 2 にも `source.fixAll.tsuzuri` が返って落ちた |
| 12 | `src/lsp.rs` の `a_cancelled_code_action_stops_before_its_first_verification`: 1 回目から取り消しを返す関数を渡すと、関数はちょうど 1 回呼ばれて -32800（範囲に修正案がある場合と、`fix_all` だけの場合） | `code_actions` が関数を受け取らないので型検査で落ちる（HEAD は計算し終えてから -32800 を返すので、応答の種類では区別できない） |
| 12 | `src/fix.rs` の `a_cancelled_run_restores_the_texts`: 2 回目のラウンドの前の取り消しで `Stop::Cancelled`、本文は元のまま、適用の記録なし | `Stop::Cancelled` がないので型検査で落ちる |

既存のテストの移行（段 11）: 要求の後に同じ文書の `didChange` を先送りで続ける 7 つ（`completion_offers_fields_members_locals_and_keywords`、`signature_help_inside_brackets_that_an_earlier_line_opened`、`stale_fallbacks_hide_private_declarations_of_other_modules`、`inlay_hints_show_implicit_copies`、`namespaces_and_using_resolve_definitions_completions_and_tokens`（2 か所）、`opt_in_matrix_module_completes_and_hovers_when_named`、`opt_in_view_and_tensor_modules_complete_when_named`）を、各応答を待ってから次を送る `stepwise` に移した。期待値は変えていない。
先送りのままだと新しい待ち行列で 7 つとも落ちる（-32801 など。`pipelined-on-new-serve.out`）ことを確かめた。`tests/lsp.rs`・`tests/lsp_sessions.mjs`・`vsc/src/test/extension.test.ts` の既存の期待値は変えていない。

### 確認（macOS 27.2、Apple Silicon、ほかのワークツリーと共有する機械、Node 24.22.2、VS Code 1.103.2）

T0・T1・T2 の結果は下の「レビューの前の確認」と「独立レビューの指摘と対応」に書く。

- `cd vsc && npm run test:unit`: 20 passed、0 failed。`npm run check-types`・`npm run lint`: エラーなし。
- 拡張の統合テスト（`ELECTRON_RUN_AS_NODE` と `VSCODE_*` を外す）: 16 の段の `VS Code: …` と `All Tsuzuri VS Code integration checks passed.`（`vsc-new-compiler-2.out`）。
  コンパイラは、同梱のツールチェーンを作り直さず、テストのワークスペースの設定 `tsuzuri.compilerPath` をこのブランチの `target/release/tsuzuri` にして動かした（`vsc/scripts/test-extension.mjs` を一時的に変えた。コミットしていない）。`--user-data-dir` は、ワークツリーのパスが長く IPC の socket のパスの上限に近いので、短い `target/scratch/g39/ud` にした。
  このワークツリーには `vsc/resources/codelldb.vsix` がなく、Debug の段が CodeLLDB を gallery から入れようとして落ちたので、共有の `target/vsc-downloads` の VSIX（SHA-256 が `vsc/scripts/toolchain.mjs` の値と同じ）を写した。
- `node tests/lsp_sessions.mjs target/debug/tsuzuri`: 4 つのセッションが成功（`t2-sessions.out`）。
- `node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/diagnostics.md _tsuzuri/language-reference/compiler/usage.md`: 2 ページ、137 リンク、5 例、4 回の native の実行（O0/O3）、成功（`check-docs.out`）。

### レビューの前の確認（コードは `d7801925`）

- T0: `cargo fmt --all -- --check` は差分なし、`cargo clippy --locked --all-targets -- -D warnings` は警告なし。`cargo nextest run --locked --test lsp --lib -E 'binary(lsp) | test(lsp::) | test(fix::)'` は 96 passed（`tests/lsp.rs` は 63 件。前は 52 件で、11 件を足した）。
- T1（`cargo nextest run --locked`）: Phase 2 の後（`7b1872e8`）3357 passed・4 skipped、Phase 3 と文書の後（`a8a7063d`）3364 passed・4 skipped、最後（`d7801925`）3364 passed・4 skipped。
- T2: T1 に加えて、`node tests/lsp_sessions.mjs target/debug/tsuzuri` は 4 つのセッションが成功、上の `check-docs`、`cd vsc && npm run test:unit`（20 passed）と統合テスト（`d7801925` の `target/debug/tsuzuri` を `tsuzuri.compilerPath` にして、16 の段と `All Tsuzuri VS Code integration checks passed.`。`vsc-final-dev.out`）、`node tests/platforms.mjs` は成功、`node scripts/check-links.mjs`（`--into _features/README.md` ほかも）は 0 broken、`node scripts/check-guide-lite.mjs` は 88 references・0 failed、`git diff --check` は差分なし。`src/runtime/` は変えていないので `check-runtime-includes.sh` は動かしていない。

### 段 13（計測）

原因 5 の計測を、記録のときと同じ harness と同じプロジェクトで、直す前と後に取った（2026-11-20）。

- **比べた binary**: before は `target/perf/G39/tsuzuri-before`（main `295ffcb4` の release、PB08 の before と同じ binary）、after はこのブランチ `a01e2665` の release（`target/perf/G39/tsuzuri-after`）。
- **環境**: Apple M3（8 コア）、macOS 27.2.1、VS Code 1.103.2（`vsc/.vscode-test`）、拡張は main の作業ツリーの `vsc/`（0.1.8 の dist）。両方で同じ拡張を使い、言語サーバーだけを替えた。
- **負荷**: ほかのワークツリーの作業は止めていた。load average は 3.0〜8.4。Spotlight、ブラウザー、エディター、ウイルス対策が動いていた。
- **harness**: `target/scratch/g39/verify-lsp-latency-robustness/vscode/`。`runner.mjs` と `scenario.test.js` は記録のときのまま。`m13.mjs` が 4 つの組を 9 巡し、各巡で before と after の順を入れ替える。
- **生データ**: `target/perf/G39-m13/`（`results.json`、`summary.json`、各実行の `*.result.json` と `*.traffic.jsonl`、`m13.log` に各実行の前後の負荷）。

時間はどれも、最初の診断の後の編集から測った（ms、9 回の中央値と最小〜最大）。

| 組 | before | after | after/before |
| --- | ---: | ---: | ---: |
| 215 KB・6 ファイル（`bigsix`、`W1001` が 6 つ）を 1 秒に 1 回直す、電球あり: 最後の編集から診断が消えるまで | 5324（5221〜12405） | 2923（1808〜3022） | 0.55 |
| 同じ、電球なし | 4581（4445〜4809） | 2997（2833〜3077） | 0.65 |
| 1 ファイル（`small`、`W1001` が 3 つ）で、警告に 600 ms カーソルを置いてから 1 つ直す、電球あり: その警告が消えるまで（3 つの修正それぞれ） | 1467（1417〜1524。24 個） | 675（638〜718。27 個） | 0.46 |
| 同じ、電球なし | 565（523〜592。27 個） | 558（515〜589。27 個） | 0.99 |

- **215 KB・電球あり**: before は 2 つの山に分かれた。9 回のうち 5 回が 5.2〜5.3 秒、4 回が 11.9〜12.4 秒で、原因 5 の連鎖（取り消された要求と古い版を順に計算する）が閾値で起きる。after は 9 回とも 3.0 秒以下だった。
- **215 KB・電球なし**: 0.65 倍になった。版ごとの解析を、後の版のある間は飛ばし（-32801）、取り消された要求を計算しないため。
- **1 ファイル・電球あり**: 0.46 倍になった。電球の自動の要求が `fix_all` を計算しなくなったため（D11）。
- **1 ファイル・電球なし**: 変わらない。待たされる要求がなく、残るのは 1 回の解析の費用（200 ms の debounce と再解析）。これを縮めるのは PB08。
- **除いた 1 回**: 1 ファイル・電球ありの before の 1 回（`bSn4`）を除いた。VS Code の `type` コマンドが文書を変えず（`doc.version` が 1 のまま、traffic に `didChange` がない）、harness の操作が効かなかった回で、言語サーバーの計測ではない。

このブランチは 1 回の解析の速さを変えない（それは PB08）。上の差は、待ち行列・取り消し・`ContentModified`・電球の `fix_all` を省いたことによる。CHANGELOG には速さの数値を書いていない。

### チケットから外れた判断

1. **待ち行列にない ID の取り消しを、そのとき捨てずに直近の 1024 件だけ残す。** 仕様 13 は「待ち行列にも処理中にもない id の取り消しは捨て」と書く。だが `tests/lsp_sessions.mjs` は取り消しを要求より先に送り（`notify("$/cancelRequest", { id: nextId })` の後に `request`）、-32800 を期待している。主ループが取り消しを読んだ時点で要求がまだ届いていないことがあるので、捨てるとこの既存の検査が時々落ちる（停止条件の「既存の期待値を変えないと通らない」を避けた）。
   答えた要求の取り消しは捨てる（`Queue::answer` が直近 1024 件の答えた ID を覚え、後から届いた取り消しを集合から消す）。それ以外の待ち行列にない取り消しは、直近 1024 件を残して古いものから集合から消す。件数の上限で新しい取り消しを拒むことはないので、受け入れ条件（取り消しが効かなくならない）と段 11 の 1100 個の検査は満たす。
   どちらの記録も、Copilot の 2 回目のレビューで、ID が 256 バイト以下のものだけにした（逸脱 11）。同じ ID の待ち行列にない取り消しは 1 件にし、最後に届いた位置に置く（Copilot の 2 回目のレビューの 3）。
2. **`fix::run` の取り消しは、`Stop` の新しい値 `Cancelled` で返す。** 設計は「ラウンドの間に取り消しを確かめる関数を受け取る」だけを決めている。誤り（`SourceError`）にすると診断コードが要るので、報告の停止の理由にした。最初の解析の前の確認は `fix_all` が読み込みの前に行い、`run` は各ラウンドの解析の前に確かめる。CLI は取り消さないので、`--json` の `stop` に `cancelled` は出ない。
3. （取り下げた）最初の実装は、watcher の事象の絞り込みを拡張の 2 つの watcher の両方に掛け、「モジュールの発見はどちらも読まない」とした。発見は知識グラフの印のない `target/tz/` の下のソースを読むので、この前提は誤りだった（レビューの指摘 5）。設計のとおり、`**` の削除の watcher だけが送るパスに絞った。
4. **-32801 は要求だけに使う。** ID を持つ text sync（`rejects_documents_over_the_source_limit` のような `didOpen` の要求）は、今までどおり反映して答える。
5. **URI を作れないパス（UTF-8 でないパス）には送らない。** 前は `"uri": null` の通知を送っていた。
6. **`vsc/README.md`・`vsc/CHANGELOG.md`（`## Unreleased`）も更新した。** チケットの「ドキュメント」は Phase 2・3 に拡張の文書を挙げていないが、拡張の利用者から見える振る舞い（消える診断、拡張の watcher）が変わるため。
7. **背圧は、本文の長さに加えて、メッセージごとに 1 KiB（`MESSAGE_COST`）を数える。** 設計（段 11）は、まだ処理していないメッセージの本文の合計にかけると書く。本文だけを数えると、本文が空か小さいメッセージを際限なく溜められる（下の Copilot のレビューの 2）。大きいメッセージの上限（16 MiB の本文 4 つ分）はほとんど変わらない。
8. **同じサーバーの別の根の記録にまだある URI には `[]` を送らない（D5 の補足）。** D5・仕様 7 は、送らなくなった URI に `[]` を送ると書く。クライアントは URI の一覧を丸ごと置き換えるので、1 つのサーバーの複数の根が同じ URI に出すとき（両方が依存する根の外の `Tsuzuri.toml` の読み込みのエラー）、片方の `[]` がもう片方の診断を消す（下の Copilot のレビューの 3）。最後に出していた根が出さなくなったときに消す。D6 は変わらない。
9. **拡張の `**` の削除の watcher が除くのは、`.` で始まる名前と `target/tz/kg/` だけ。** 設計の表（Phase 1 の新しい購読、Phase 2 の「除く場所は Phase 1 と同じ」）は `target/tz/` の下を除くと書き、理由を「モジュールの発見が読まず」とするが、発見は知識グラフの印のない `target/tz/` の下を読む（下の Copilot のレビューの 5）。コンパイラが `target/tz/` で入れ替えるのは知識グラフだけなので、そこだけを除く。`tsuzuri-build` の結果の消し方（Phase 1）も同じ関数を使う。
10. **text sync は、同じ文書の後の text sync が置き換えるものも、飛ばさずに届いた順に反映して検査する。** 仕様 12 と設計（Phase 3 の変更の `serve`）は、同じ文書の `didChange` を、その間に計算する要求がないときにまとめて最後の 1 つだけを反映すると書く。まとめると、後の版が拒まれたとき（4 MiB を超えるなど）に受け付けた版を失い、ほかの文書の変更を、開いているバッファの合計の上限（32 MiB）に対して置き換えられた本文で数える（下の Copilot の 2 回目のレビューの 4）。
    まとめなくても途中の版は解析しない。反映は本文を写して根を予定に入れるだけで、解析（refresh と診断の送信）は計算する要求か待ち行列が空になったときにしか動かず、まとめていた 2 つの版の間にはそのどちらもない。仕様 12 の「途中の版の診断を 1 つずつ送らない」、受け入れ条件、`a_request_for_a_replaced_text_gets_content_modified` の期待は変わらない。
11. **ID（JSON の表記）が 256 バイト（`MAX_REMEMBERED_ID`）を超える要求は、答えた ID と待ち行列にない取り消しの記録に入れない。** 仕様 13 と設計は、取り消しの集合の大きさを背圧で抑えると書くが、処理し終えたメッセージの ID の記録は背圧の外にあり、件数の上限（1024）だけではメッセージの上限まで長い ID で GiB 単位を持ち続ける（下の Copilot の 2 回目のレビューの 1）。
    そうした ID の答えた要求への遅れた取り消しは、待ち行列にない取り消しとしてすぐ捨てるので、今までどおり効かない。失うのは、そうした ID で要求より先に届いた取り消しだけ（その要求は計算する）。待ち行列の要求と処理中の要求の取り消しは、ID の長さに関わらず効く。
12. **クライアントの根は、`initialize` のときに加えて refresh のたびに正準化し直す。** 設計（Phase 2 の変更の `Session`・`initialize`）は、`initialize` のときにクライアントの根の URI と正準化したパスの組を記録すると書く。シンボリックリンクの根は消されたり向き先を変えられたりするので、一度だけの正準化では、前の向き先の診断をリンクの表記で出し続ける（下の Copilot の 2 回目のレビューの 2）。
    前のディレクトリをどのクライアントの根も指さなくなったら、Copilot の 1 回目のレビューの 1 の消した根と同じく、そのディレクトリの解析を捨てて記録の URI に空の一覧を送る。新しいディレクトリは予定に入れる。
    Copilot の 3 回目のレビューで、ファイルの変更の通知でも、変わったパスを根に当てる前に正準化し直すようにした（下の 3 回目の 2。記録も開いている文書もない根では、通知のほかに予定に入れるものがない）。
13. **`Inbox` の取り消しは、ID の集合でなく ID ごとの件数で持つ。** 設計（Phase 3 の変更の受信のスレッドと取り消しの集合）は、受信のスレッドが読んだ ID を「今と同じく共有の集合に入れる」と書く。集合では、主ループが 1 つの取り消しを手放すとき（押し出す、遅れたものとして捨てる、答える）に、受信のスレッドが先に読み、主ループがまだ待ち行列へ移していない同じ ID の取り消しの記録まで消える（下の Copilot の 3 回目のレビューの 3）。
    件数にし、主ループは自分が持つ取り消し（`Queue::unmatched` の 1 件と、待ち行列の要求の分の `Queue::held`）を手放すときに、ちょうどその件数を減らす。どの取り消しが効くかの規則（仕様 13、逸脱 1・11）は変わらない。

### 残る問題

- `ci:full`（`windows.yml`・`vscode.yml`）は push していないので未実行。Windows の `\\?\` の根と `file:///c%3A/...` の根の URI の表記（`uri_under`）は、手元では確かめていない（`windows_file_uris_round_trip_drive_letters_and_canonical_paths` は変えていない）。シンボリックリンクの根の検査は Unix だけ。
- 根の外のファイルの読み込みのエラーは、そのファイルの変更がこのクライアントに届かないので、根の次の解析まで残る（仕様 11 のとおり）。
- どのクライアントの根にも入らない文書（親のディレクトリを根とするもの）の D9 は、その親のディレクトリで判定する（今と同じ根の決め方）。
- クライアントの根を正準化し直すのは、refresh のときとファイルの変更の通知のとき（逸脱 12。通知の分は Copilot の 3 回目のレビューの 2）。根のシンボリックリンクを変えてから、そのどちらかまでの間に届いた text sync と要求は、前の向き先で文書の根を決める（`project_root`）。その根の refresh は最初に根を正準化し直し、新しい向き先を予定に入れ、前の向き先を外す。
- （Copilot の 3 回目のレビューの 3 で閉じた）ID を使い回すクライアントで、答えた要求への遅れた取り消し、同じ ID の新しい要求、その取り消しが続けて届き、受信のスレッドが 3 つ目を記録してからチャンネルへ送るまでの間に、主ループが 1 つ目を移して記録を消し、新しい要求を始めると、その取り消しが効かなかった。取り消しを件数で数え、1 つ目を捨てるときは 1 つ目の分だけを減らすので（逸脱 13）、3 つ目の記録は残り、新しい要求は取り消される。
- 1 つのサーバーの複数の根が同じ URI に違う一覧を出すとき（入れ子の根。G40）、出さなくなった根の `retire` は、別の根の記録にあるその URI に `[]` を送らない（逸脱 8）。その URI には最後に送った一覧が残り、それが出さなくなった根のものなら、別の根の次の解析まで残る（記録のある根なので、次のファイルの変更の通知で解析し直す）。根の外の読み込みのエラーは、どの根から見ても同じファイルの同じ誤りなので、この場合に当たらない。
- 壊れた `Tsuzuri.toml` で、`tsuzuri check --json` の `line`・`column` が span（5 行目の 58..63）でなく `1`・`1` になる（独立レビューの観察。G39 より前からの CLI の振る舞い）。言語サーバーの範囲は span から作るので、受け入れ条件の「CLI の `--json` と同じ位置」は span で満たす。CLI の行と列は別に扱う。
- PR #120（G40）の `ci:full` で、Windows x64 の `vscode.yml` の `npm run test:installed`（同梱の VSIX を入れた VS Code）だけが 1 回、G39 Phase 1 の検査「テストの実行が言語サーバーの警告の写しを出さない」で、テストの実行の直後の `E2006`（失敗したテストの結果）が見つからずに落ちた（run 44720310026、2026-11-20）。同じホストの直前の `npm test` と、落ちた job の再実行は通った。結果を消したもの（テストの実行が根の下に作って消すファイルの遅れた事象など）は確かめられていない。再び起きたら、そのときの拡張の出力と事象を記録して調べる。

### 独立レビューの指摘と対応（2026-11-20）

`d7d03bf0` への独立レビューの指摘 5 件（major 1、minor 4）を、それぞれコードで確かめた。5 件とも採用した。記録は `target/scratch/g39/fix/`・`target/scratch/g39/fix2/`。

1. **major: 受け入れ条件を満たさないまま done にし、`_completed/` へ移していた。** 段 13 の計測（「段 13 の計測を、直す前と後でこのチケットに記録している」）と `ci:full` の `vscode.yml`・`windows.yml` が残っている（GUIDE §0: すべての受け入れ条件を確かめてから done）。
   採用。チケットを `_features/` へ戻し、状態と `_features/README.md` の行を doing（Phase 1 done。Phase 2 と Phase 3 の段 11・12・14 は実装済み。段 13 の計測と `ci:full` が残る）にした。チケットの中の相対リンク（G40、PB08）を戻した。段 13 は GUIDE §14 のとおり、ほかの作業のない機械で取る。この機械はほかのワークツリーと共有しているので、ここでは取らない。
2. **minor: `a_linked_root_keeps_the_clients_spelling` の `Sub/Util.tz` の部分が、「変更の通知が別の根を始めない」誤りを見つけられない。** 設計の表が警告する誤り（`changed_roots` で `uri_path(uri)` をそのまま `project_root` に渡す）を入れても、D9 が、正準化していないディレクトリを根とする誤った解析の診断を隠すので、テストの診断の検査は通った。
   採用。通知の後に `workspace/symbol` の `util` がちょうど 1 件（`link/Sub/Util.tz`）であることを確かめるようにした。その変異では 2 件になって落ち（`fix/mutant-subroot.out`）、このブランチでは通る。テストの表の行も直した。
3. **minor: 仕様 9 からの逸脱。変わったパスが存在すると、親のディレクトリでなくパス自身を正準化していた（`canonical_existing`）。** 根の中に作った、根の外を指すソースのシンボリックリンクが、どのクライアントの根の外へ解決され、事象が捨てられた。
   採用。`document_path` と同じく親から始めるようにした。`a_link_created_in_the_root_is_placed_by_its_directory`（Unix）を足した: 根の文書を閉じて何も送っていない状態で `app/Ext.tz`（`other/Ext.tz` へのリンク）を作って通知すると、根を解析し、発見の `E1011` が `app/Ext.tz` に届き、`other/Ext.tz` には何も届かない。直す前は `app/Ext.tz` に何も届かず落ちた（`fix/before.out`）。
4. **minor: 応答の後に ID を使い回すクライアントの取り消しが捨てられる（退行）。** `Queue::push` が待ち行列の要求より先に答えた ID を見ていたので、使い回した ID の要求の取り消しを、前の要求への遅れた取り消しとして捨てた。
   採用。取り消しは待ち行列の要求を先に見て、答えた ID と同じ ID の要求が届いたら、その ID を答えた ID の記録から外すようにした。確かめる中で、同じ原因の別の場合も見つけた: 答えた要求への遅れた取り消し、同じ ID の新しい要求、その取り消しが 1 回の `try_recv` の間に揃うと、1 つ目を移すときの `inbox.forget` が、受信のスレッドが 3 つ目のために入れた ID を消す。待ち行列の要求の取り消しを移すときに、ID を集合へ入れ直す（`Inbox::cancel`）ようにした。
   `a_reused_request_id_can_be_cancelled`: ID 7（使い回して取り消す）は `d7d03bf0` で結果が返って落ち（`fix/before.out`）、ID 8（遅れた取り消しの後に使い回して取り消す）は `4c16e7dc` で結果が返って落ちた（`fix2/before.out`）。ID 9（遅れた取り消しの後に使い回し、取り消さない）は結果が返ることを確かめ、遅れた取り消しが新しい要求を取り消さないことを見る。
   指摘の後半（前の要求の `Queue::answer` が新しい要求の取り消しも消す）は直していない。`answer` は応答を書く前に呼ぶので、応答を受けてから ID を使い回すクライアントの新しい取り消しは、その時点ではまだ届いていない。届くのは答えていない ID を使い回す場合だけで、JSON-RPC が許さない。残る狭い競合は上の「残る問題」。
5. **minor: 逸脱 3 の前提が誤りで、`target/tz/` の下のソースの事象が届かなくなっていた。** モジュールの発見が飛ばすのは `.` で始まる名前、パッケージの根、知識グラフの印のあるディレクトリだけ（`src/driver.rs` の発見）で、ほかの `target/tz/**/*.tz` は読む。
   採用。`vsc/src/core.ts` に `notifiesServer` を足し、middleware は `.tz`・`.tt`・`.tc`・`.toml` の事象をそのまま送り、それ以外のパス（`**` の削除の watcher だけが送るもの）を `affectsRoot` に通すようにした（設計のとおり）。`vsc/src/test/core.test.ts` に検査を足した: 直す前の `affectsRoot` では `/w/app/target/tz/gen/Gen.tz` で落ちた（`fix/unit-before.out`）。拡張のコメントと逸脱 3 を直した（逸脱 3 は取り下げ）。

数えない観察の 2 つのうち、マニフェストの誤りの CLI の行と列は上の「残る問題」に書いた。VS Code の統合テストが段 3 の後で 14 分止まった 1 回（両方の言語サーバーは入力を待って止まっていた）は、下の確認の統合テストでは再現しなかった。`vscode.yml` で見る。

#### 指摘への対応の後の確認（コードは `a01e2665`）

- T0: `cargo fmt --all -- --check` は差分なし、`cargo clippy --locked --all-targets -- -D warnings` は警告なし。`cargo nextest run --locked --test lsp --lib -E 'binary(lsp) | test(lsp::) | test(fix::)'` は 98 passed（`tests/lsp.rs` は 65 件。指摘 3 と 4 で 2 件を足した。`fix2/after-lsp.out`）。待ち行列と指摘 2〜4 のテスト 6 件（`-E 'test(a_reused_request_id) | test(a_link_created) | test(a_linked_root) | test(cancel) | test(content_modified)'`）を 6 回続けて動かし、6 回とも通った。
- T1（`cargo nextest run --locked`）: 3366 passed・4 skipped（`fix2/t1.out`）。
- T2: `cargo build --locked` の後、`node tests/lsp_sessions.mjs target/debug/tsuzuri` は 4 つのセッションが成功（`fix2/sessions.out`）。`node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/diagnostics.md _tsuzuri/language-reference/compiler/usage.md` は 2 ページ、137 リンク、5 例、4 回の native の実行（O0/O3）で成功（`fix2/check-docs.out`）。`cd vsc && npm run test:unit` は 21 passed（指摘 5 で 1 件を足した）、`npm run check-types`・`npm run lint` はエラーなし（`fix2/vsc-unit.out`）。
  拡張の統合テスト（`ELECTRON_RUN_AS_NODE` と `VSCODE_*` を外す）は、16 の段と `All Tsuzuri VS Code integration checks passed.`（`fix2/vsc.out`）。同梱のツールチェーンは作り直さず、テストのワークスペースの `tsuzuri.compilerPath` を `a01e2665` の `target/debug/tsuzuri` にした。そのために `vsc/scripts/test-extension.mjs` を、追跡しない写し（`--user-data-dir` を短い `target/scratch/g39/ud` にし、設定に `tsuzuri.compilerPath` を足したもの）で動かし、終わってから消した。
  `node tests/platforms.mjs` は成功、`node scripts/check-links.mjs`（`--into _features/README.md` も）は 0 broken、`node scripts/check-guide-lite.mjs` は 88 references・0 failed、`git diff --check` は差分なし。`src/runtime/` は変えていないので `check-runtime-includes.sh` は動かしていない。

### Copilot のレビュー（PR #119、2026-11-20）

GitHub Copilot の 1 回目のレビュー（`a01e2665` のコード）の 5 件を、それぞれコードで確かめた。5 件とも本当の誤りで、すべて直した。各修正は、直す前に落ちる回帰テストと一緒に 1 つずつコミットした。記録は `target/scratch/g39/copilot1/`（`before.out` は 4 つの Rust のテストを直す前のコードで動かしたもの）。

1. **4638745418（`src/lsp.rs:551`）: 消した・移したクライアントの根を、親のディレクトリとして読み込む。** 直した。
   `ClientRoot` は根を `initialize` のときに一度だけ正準化するので、根のディレクトリを消した後も、その下の変更の通知と `published` の記録がその根を `schedule_root` する。`Project::load_with_overlays` はディレクトリでないパスには親のディレクトリを読む（`src/driver.rs:2400-2408`）ので、隣のプロジェクトを索引に入れ、その読み込みのエラーを送っていた（D9 の除外は根のディレクトリで判定するので、解析の診断は漏れていなかった）。
   `refresh` は、根がディレクトリでなければ、読み込まずにその根を `projects`・`good` から捨て、空の `Sent` で `retire` する（記録の URI に `[]`）。読み込みのエラーは送らない（消えた根に、直すファイルはない）。
   テスト `a_removed_root_is_not_loaded_through_its_parent`: 根 `app` を消して通知すると、`workspace/symbol` に隣の `other/Other.tz` の `beside_the_root` が出ず、`app/Helper.tz` に `[]` が届き、隣に置いた `Stray.old.tz` を通知した後も、`app` の下の空の一覧のほかは何も送られない。直す前は `beside_the_root` が返って落ち（`before.out`）、記号の検査を外すと `Stray.old.tz` に `E1011` が届いて落ちた（`before-1-phase2.out`）。
2. **4638745452（`src/lsp.rs:3357`）: 本文が空のメッセージが背圧に数えられない。** 直した。
   `Content-Length: 0` の枠は JSON の誤りとして待ち行列に入るが、本文の長さ 0 しか数えないので、主ループが計算している間に際限なく溜まる（本文の小さいメッセージも、待ち行列の項目に比べてほとんど数えない）。空の枠を拒む案は、今の振る舞い（`-32700` で答えて続ける）を変え、小さいメッセージの場合を残すので採らなかった。
   受信のスレッドは、メッセージごとに本文の長さと `MESSAGE_COST`（1 KiB。チャンネルと待ち行列の項目の分）を数え、その値を `InputEvent::Message` に入れる。主ループは処理したときに同じ値（`Queued::bytes`）を引くので、数は釣り合う。どれだけ小さいメッセージでも、約 65,536 個で受信のスレッドが待つ（逸脱 7）。
   テスト `src/lsp.rs` の `empty_messages_count_against_the_readers_limit`: 空の枠 100,000 個を `read_input` に渡すと、`Inbox` の数が `MAX_QUEUED_BYTES` を超えたところで受信のスレッドが待ち、終わりまで読まない。直す前は `the reader read all 100000 empty messages without waiting` で落ちた。
3. **4638745474（`src/lsp.rs:514`）: ある根の `retire` が、別の根がまだ出している URI の一覧を消す。** 直した。
   1 つのサーバーが複数の根（`workspaceFolders`）を受け持つと、両方が依存する根の外の壊れた `lib/Tsuzuri.toml` の `E0002` を両方の根が送り、それぞれの記録に入る。片方が依存を外すと、その `retire` が `[]` を送り、もう片方がまだ読み込めない理由が消えた（ファイルの変更の通知は記録のある根をすべて解析し直し、その順はパスの順なので、外した根が後に解析されると起きる）。
   `retire` は、別の根の記録にまだある URI に `[]` を送らない。最後に出していた根が出さなくなったとき、その根の `retire` が消す（逸脱 8）。D6（読み込めない根のほかのファイルの診断を消す）は変わらない。D9 で解析の診断はその根の中のファイルにだけ送るので、別の根の記録と重なるのは、根の外の読み込みのエラーと、入れ子の根（G40。「残る問題」）だけ。VS Code の拡張は根ごとに別のクライアントを起動するので、この場面は複数の根を 1 つのサーバーに渡すエディターで起きる。
   テスト `a_load_error_that_another_root_reports_stays`（`live::Server::start_with` を足し、`workspaceFolders` で 2 つの根を渡す）: 根 `one`・`two` が `../lib` に依存し、`lib/Tsuzuri.toml` が壊れている。`two` が依存を外すと（`one` が先に解析される）、`lib/Tsuzuri.toml` の `E0002` が残り、`one` も外すと `[]` が届く。直す前は `two` の後に `[]` になって落ちた（`left: [] right: ["E0002"]`）。
4. **4638745506（`src/lsp.rs:3268`）: 要求より先に届いた取り消しが、その要求が届いた後も「待ち行列にない取り消し」に残る。** 直した。
   cancel(A)、request(A)、存在しない ID の取り消し 1024 個が続くと、A が最も古いものとして外れ、`inbox.forget(A)` が取り消しを消すので、待ち行列の A を計算していた。
   `Queue::push` は、要求が届いたら、その ID を `unmatched` から外す（取り消しはその要求のものになり、答えたときに `Queue::answer` が消す）。
   テスト `src/lsp.rs` の `a_cancellation_before_its_request_survives_later_unknown_ones`: `Queue` と `Inbox` に、受信のスレッドと同じ順で cancel(5)・request(5)・存在しない ID の取り消し 1025 個を入れても、5 の取り消しが残る（時間に依らない）。直す前は `assertion failed: inbox.is_cancelled("5")` で落ちた。
5. **4638745548（`vsc/src/core.ts:98`）: `target/tz/` の下のソースのフォルダーの削除が言語サーバーに届かない。** 直した。
   モジュールの発見が飛ばすのは、`.` で始まる名前、パッケージの根、知識グラフの印（`.tsuzuri-kg`）のあるディレクトリだけで（`src/driver.rs` の発見）、`target/tz/` のほかのディレクトリは読む。`affectsRoot` は `target/tz/` の下のすべての削除を捨てていたので、`target/tz/gen`（`Gen.tz` を含む）を消しても、VS Code が送るフォルダーの 1 件の事象が言語サーバーに届かず、テストの実行の結果（`tsuzuri-build`）も消えなかった。
   `affectsRoot` は、`target/tz/kg/`（根か根の下のパッケージの知識グラフ）とその下だけを捨てるようにした（逸脱 9）。コンパイラが `target/tz/` に書くのは知識グラフと、その入れ替えの一時ディレクトリ（`.tsuzuri-<pid>-<n>`。`.` で始まる）だけ。`target/tz/` のすべての削除を送る案は採らなかった: テストの発見（`tsuzuri test --list --json`。保存のたびに動く）も知識グラフを入れ替える（失敗した発見でも書くことを、このブランチの binary で確かめた）ので、保存のたびに refresh が 1 回増える。`--kg-dir`・`TSUZURI_KG_DIR` で根の中のほかの場所に置いた知識グラフの削除は送る（refresh が 1 回増えるだけ）。
   テスト `vsc/src/test/core.test.ts`: `affectsRoot` と `notifiesServer` が `/w/app/target/tz/gen`（Windows の `C:\w\app\target\tz\gen` も）を通し、`target/tz/kg` とその下は今までどおり捨てる。直す前は `/w/app/target/tz/gen` で 21 件中 2 件が落ちた（`vsc-unit-before.out`）。

文書: `docs/architecture.md` の言語サーバーの節（根がなくなったとき、別の根の記録にある URI、`MESSAGE_COST`、要求が届いた取り消し）、`_tsuzuri/language-reference/compiler/diagnostics.md` の「LSP での表示」（根のフォルダーの削除、別の根が出している診断）、`_tsuzuri/language-reference/compiler/usage.md` の `lsp`（背圧の数え方）。`vsc/README.md`・`vsc/CHANGELOG.md` は、除く場所を書いておらず、「プロジェクトの下のファイルやフォルダーの削除」の記述がこの修正で正しくなるので変えていない。

#### 対応の後の確認（コードは `c7ae761d`）

- T0: `cargo fmt --all -- --check` は差分なし、`cargo clippy --locked --all-targets -- -D warnings` は警告なし。`cargo nextest run --locked --test lsp --lib -E 'binary(lsp) | test(lsp::) | test(fix::)'` は 102 passed（`tests/lsp.rs` は 67 件。指摘 1 と 3 で 2 件、`src/lsp.rs` の `mod tests` に指摘 2 と 4 で 2 件を足した。`t0-area.out`）。待ち行列と新しいテストの 12 件（`-E 'test(a_reused_request_id) | test(a_link_created) | test(a_linked_root) | test(cancel) | test(content_modified) | test(removed_root) | test(another_root) | test(empty_messages)'`）を 6 回続けて動かし、6 回とも通った（`repeat.out`）。
- `cd vsc && npm run test:unit` は 21 passed（指摘 5 は既存の 2 つのテストに場合を足した）、`npm run check-types`・`npm run lint` はエラーなし。
- T1（`cargo nextest run --locked`）: 3370 passed・4 skipped（`t1.out`）。
- T2: `cargo build --locked` の後、`node tests/lsp_sessions.mjs target/debug/tsuzuri` は 4 つのセッションが成功（`sessions.out`）。`TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/diagnostics.md _tsuzuri/language-reference/compiler/usage.md` は 2 ページ、137 リンク、5 例、4 回の native の実行（O0/O3）で成功（`check-docs.out`）。`node scripts/check-links.mjs`（`--into _features/README.md` も）は 0 broken、`node scripts/check-guide-lite.mjs` は 88 references・0 failed、`git diff --check` は差分なし。
  拡張の統合テスト（`ELECTRON_RUN_AS_NODE` と `VSCODE_*` を外す）は、前の確認と同じ 15 行の `VS Code: …` と `All Tsuzuri VS Code integration checks passed.`（`vsc.out`）。前と同じく、テストのワークスペースの `tsuzuri.compilerPath` を `c7ae761d` の `target/debug/tsuzuri` にした追跡しない写し（`--user-data-dir` は `target/scratch/g39/ud`）で動かし、終わってから消した。
  `src/runtime/` は変えていないので `check-runtime-includes.sh` は動かしていない。段 13 の計測は取り直していない（この修正は、待ち行列に入ったメッセージの数え方と、まれな取り消しと根の場合だけを変える）。`ci:full` は未実行。

#### 2 回目（`9739c6f0` のコード）

GitHub Copilot の 2 回目のレビューの 4 件（2026-11-20）を、それぞれコードで確かめた。4 件とも本当の誤りで、すべて直した。各修正は、直す前に落ちる回帰テストと一緒に 1 つずつコミットした。記録は `target/scratch/g39/copilot2/`（`before.out` は 5 つのテストを `9739c6f0` のコードで動かしたもの。`long_ids_are_not_remembered` の最初の版は、待ち行列に残った取り消しを取り出さないテスト自身の誤りで落ちたので、直したテストを同じコードで `before-long-ids.out` に動かし直した）。

1. **4639668076（`src/lsp.rs:3325`）: 処理し終えた要求の ID の記録が、件数でしか抑えられていない。** 直した。
   答えた ID（`answered` と `recent` に 1 つずつ写す）と待ち行列にない ID の取り消し（`unmatched` と `Inbox` の集合）は、それぞれ直近の 1024 件を残す。文字列の ID はメッセージの上限（16 MiB）まで長くできるので、長い ID の要求を 1 つずつ送ると、待ち行列が空になった後も GiB 単位を持ち続けた（答えた ID だけで 1024 × 16 MiB × 2）。
   ID（`Value::to_string` の表記）が 256 バイト（`MAX_REMEMBERED_ID`）を超える要求は答えた ID に記録せず、そうした ID の待ち行列にない取り消しはすぐ集合から捨てる（逸脱 11）。記録は 1 つにつき 1024 × 256 バイト以下になる。バイト数の合計で抑える案は採らなかった: 予算を超える 1 つの ID が、記録のほかの ID をすべて押し出す。
   テスト `src/lsp.rs` の `long_ids_are_not_remembered`: 64,002 バイトの ID の要求 64 個を答え、それぞれの遅れた取り消しと、存在しない長い ID の取り消しを入れた後、`answered`・`recent`・`unmatched`・`Inbox` の集合に残る ID の長さの合計が 0。短い ID は今までどおり記録する（答えた 7 への遅れた取り消しを捨て、要求より先に届いた 8 の取り消しが効く）。直す前は `left: 16384512 right: 0` で落ちた（`before-long-ids.out`）。
2. **4639668130（`src/lsp.rs:420`）: シンボリックリンクの根を消したり向き先を変えたりした後も、前の向き先の診断をリンクの表記で出す。** 直した。
   `ClientRoot` は根を `initialize` のときに一度だけ正準化していた。`link`（→ `real`）を根にして文書を閉じた後に `link` を消すと、`published` の記録が `real` を解析し直させ、`real` はディレクトリとしてあるので読み込まれ、`link/Helper.tz` のエラーを、もうない URI に送り直していた（記録と同じ URI なので `[]` も送らない）。向き先を変えたときも、前の向き先の診断を新しい向き先のファイルの表記で出していた。
   `refresh` は最初にクライアントの根をすべて正準化し直す（`Session::resolve_client_roots`。refresh ごとに根の数だけの `canonicalize`）。前と違うディレクトリに解決した根はそれを持ち、新しいディレクトリを予定に入れる。前のディレクトリをどのクライアントの根も指さなくなったら、その解析を捨て、予定から外し、記録の URI に空の一覧を送る（1 回目の 1 の消した根と同じ扱い。別の根がまだ指していれば、その根の表記で解析し直すよう予定に入れる。逸脱 12）。
   テスト `a_removed_or_repointed_root_link_is_resolved_again`（Unix）: `link`（→ `real`。`Helper.tz` に `E1003`）を根にして文書を閉じた後、`link` を消して通知すると `link/Helper.tz` に `[]` が届き、`real/Helper.tz` には何も届かない。別のサーバーで、`link` を `moved`（`Helper.tz` に `E1002`）へ向け直して通知すると、`link/Helper.tz` が `E1002` になり、`real/`・`moved/` の表記には何も届かない。直す前は、消した場合に `link/Helper.tz` へ `E1003` が送り直されて落ちた（`assertion failed: server.codes(&helper).is_empty()`）。
3. **4639668188（`src/lsp.rs:3283`）: 同じ ID の取り消しが待ち行列にない取り消しに 2 つ入ると、古い方を押し出すときに、新しい方の取り消しも消える。** 直した。
   cancel(A)、cancel(A)、存在しない ID の取り消し 1023 個、request(A) の順だと、最初の A を最も古いものとして外すときの `inbox.forget(A)` が、残る 2 つ目の A の取り消しも消し、A を計算していた。
   待ち行列にない取り消しは ID ごとに 1 件にし、同じ ID の取り消しが届いたら最も新しい位置へ移す（Copilot の提案のとおり）。
   テスト `src/lsp.rs` の `a_repeated_cancellation_before_its_request_is_kept_at_its_latest_place`: 上の順と、cancel(6)・存在しない ID 1023 個・cancel(6)・存在しない ID 1 個・request(6) の順の両方で、取り消しが残る（2 つ目は、重複を捨てるだけで位置を移さない実装を見分ける）。直す前は `assertion failed: inbox.is_cancelled("5")` で落ちた。
4. **4639668253（`src/lsp.rs:3501`）: 後の `didChange` が置き換える `didChange` を、検査する前に飛ばす。** 直した。
   主ループは、同じ文書の後の `didChange` が待ち行列にあり、その間に計算する要求がない `didChange` を、反映も検査もせずに飛ばしていた（`Queue::superseded`）。後の版が拒まれると（4 MiB を超えるなど）、受け付けたはずの前の版が失われ、サーバーは版 1 のままだった。また、ほかの文書の変更を、開いているバッファの合計 32 MiB の上限に対して、飛ばした文書の古い本文で数えたので、A が縮み、B がその空きへ伸び、A がまた縮むと、B の変更を拒んでいた。
   text sync は、飛ばさずにすべて届いた順に反映して検査する（`Queue::superseded` を消した。逸脱 10）。指示にあった「反映して検査し、置き換えられた版の解析（refresh の予定と送信）だけを飛ばす」形にはしなかった。反映は本文を写して根を予定に入れるだけで、解析は計算する要求か待ち行列が空になったときにしか動かず、2 つの版の間にはそのどちらもないので、飛ばす処理がなくても置き換えられた版は解析されない。予定だけを飛ばすと、後の版が拒まれたときに、反映した前の版の解析が予定に入らない。
   テスト `a_change_before_a_rejected_one_is_kept`: 忙しいサーバーに版 2（`renamed`）、4 MiB を超える版 3、`documentSymbol` を送ると、版 3 は拒まれ（`window/logMessage`）、`documentSymbol` は `renamed` を返す。直す前は `left: ["value"] right: ["renamed"]` で落ちた。
   テスト `changes_of_two_documents_meet_the_shared_limit_in_order`: 4 MiB のバッファ 7 つ（解析が読まない `.big/` の下）と小さい `Main.tz` を開いた忙しいサーバーに、A の縮小、B の 4 MiB への伸長（要求として送る）、A の縮小を送ると、B の変更が通る。直す前は `open buffers exceed the server resource limit`（-32602）で落ちた。
   `a_request_for_a_replaced_text_gets_content_modified`（版 2 の診断は届かず、版 3 は 1 回だけ）は変えずに通る。段 13 の計測は取り直していない: 飛ばしていたのは本文の写しと予定の更新だけで、解析の回数は変わらない。

文書: `docs/architecture.md` の言語サーバーの節（根を refresh のたびに正準化し直すこと、どの根も指さなくなったディレクトリ、text sync をすべて反映すること、取り消しの記録の重複と ID の長さ）、`_tsuzuri/language-reference/compiler/diagnostics.md` の「LSP での表示」（シンボリックリンクの根を消したとき・向き先を変えたとき）。`_tsuzuri/language-reference/compiler/usage.md` の `lsp` は、「要求より前に届いた編集は、どの文書のものも届いた順に反映してから、その要求を処理します」がこの修正で正しいままで、取り消しの記録の件数も書いていないので変えていない。`vsc/` は変えていない。

#### 2 回目の対応の後の確認（コードは `503c05bd`）

- T0: `cargo fmt --all -- --check` は差分なし、`cargo clippy --locked --all-targets -- -D warnings` は警告なし（`clippy.out`）。`cargo nextest run --locked --test lsp --lib -E 'binary(lsp) | test(lsp::) | test(fix::)'` は 107 passed（`tests/lsp.rs` は 70 件。指摘 2 と 4 で 3 件、`src/lsp.rs` の `mod tests` に指摘 1 と 3 で 2 件を足した。`t0-area.out`）。待ち行列・取り消し・根のテストと新しいテストの 17 件（1 回目の確認の 12 件の式に `test(a_repeated_cancellation) | test(long_ids) | test(a_change_before_a_rejected) | test(changes_of_two_documents) | test(repointed_root_link)` を足したもの）を 6 回続けて動かし、6 回とも通った（`repeat.out`）。
- T1（`cargo nextest run --locked`）: 3375 passed・4 skipped（`t1.out`）。
- T2: `cargo build --locked` の後、`node tests/lsp_sessions.mjs target/debug/tsuzuri` は 4 つのセッションが成功（`sessions.out`）。`TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/diagnostics.md` は 1 ページ、50 リンクで成功（`check-docs.out`）。`node scripts/check-links.mjs`（`--into _features/README.md` も）は 0 broken、`node scripts/check-guide-lite.mjs` は 88 references・0 failed、`git diff --check` は差分なし。
  `vsc/` は変えていないので、`npm run test:unit` などと拡張の統合テストは動かしていない。この修正は、ふつうの診断の流れ（`tests/lsp_sessions.mjs` と統合テストが試すもの）を変えない: 変わるのは、シンボリックリンクの根を消したり向き先を変えたりしたとき、後の版が拒まれたときと共有の上限に当たったとき、取り消しの記録の端の場合だけ。`src/runtime/` は変えていないので `check-runtime-includes.sh` は動かしていない。段 13 の計測は取り直していない（上の 4）。`ci:full` は未実行。

#### 3 回目（`b4e32228` のコード）

GitHub Copilot の 3 回目のレビューの 3 件（2026-11-20。インラインの 1 件と、概要の「前に見落とした」2 件）を、それぞれコードで確かめた。3 件とも本当の誤りで、すべて直した。各修正は、直す前に落ちる回帰テストと一緒に 1 つずつコミットした。記録は `target/scratch/g39/copilot3/`。

1. **4640400937（`src/lsp.rs:592`）: 同じ一覧の前の根の refresh が外したディレクトリを、写した一覧から refresh する。** 直した。
   `workspace/symbol` とデバウンスの期限の経路は、予定の根を写した一覧を順に refresh する。前の根の refresh の `resolve_client_roots` が、リンクを消した根の前のディレクトリを外しても（解析を捨て、予定から外し、記録の URI に `[]`）、一覧に残ったそのディレクトリを次に refresh し、まだあるディレクトリを読み込んで、記号を索引に戻し、どの根にも入らない正準化した URI へ診断を送っていた（その記録も残るので、後の変更の通知のたびに解析し直す）。
   両方の経路を `Session::refresh_due` にまとめ、順が来たときに予定にない根を飛ばす。
   テスト `a_root_dropped_by_an_earlier_analysis_is_not_analyzed`（Unix）: `workspaceFolders` の 2 つの根 `a_link`（→ `a_real`）・`b_link`（→ `b_real`）で、`b_link` を消し、それぞれの根の文書を変え（変更の通知を介さずに両方を予定に入れる。通知は 2 の修正で根を先に正準化し直す）、`workspace/symbol` を送る。`b_real` の記号 `only_in_b` が返らず、`b_link/Helper.tz` に `[]` が届き、`b_real/Helper.tz` の表記には何も届かない。`b` の文書は、リンクを消した後も変えられるよう `b_real` の表記で開く。
   直す前は `only_in_b` が `b_real/Helper.tz` の URI で返って落ち（`before-1.out`）、記号の検査を後ろに回すと `b_real/Helper.tz` に `W1001` が届いて落ちた（`before-1-diagnostics.out`）。デバウンスの期限の経路は同じ関数を通る（2 つの根が同じ期限の一覧に入る順を、テストで確実に作る方法がないので、`workspace/symbol` の経路で確かめる）。
2. **概要の指摘（`src/lsp.rs:461`）: 文書を閉じた、診断のないシンボリックリンクの根の向き先を変えると、その下の変更の通知がどの根にも当たらない。** 直した。
   `changed_roots` は、変わったパスを存在する祖先で正準化して新しい向き先の下に置くが、それを前の refresh で正準化した前の向き先と比べていた。記録も開いている文書もない根では、ほかに予定に入れるものがないので、新しい向き先のエラーが出なかった。
   Copilot の 2 回目のレビューの `resolve_client_roots` を `changed_roots` の最初で呼ぶ（別の仕組みは足していない）。どの根も指さなくなったディレクトリを外す処理（解析・予定・記録。前は `refresh` が `resolve_client_roots` の後に行っていた）を `resolve_client_roots` の中へ移したので、通知の経路でも前の向き先は外れる（逸脱 12 の補足）。
   テスト `a_change_under_a_repointed_clean_root_link_analyzes_the_new_directory`（Unix）: 診断のない `link`（→ `real`）の文書を閉じた後、`link` を `moved` へ向け直し、`moved/Helper.tz`（`E1003`）の作成を通知すると、`link/Helper.tz` に `E1003` が届き、`real` の記号 `only_in_real` は返らず、送った診断の URI はすべてリンクの表記。直す前（`b4e32228`）は `link/Helper.tz` に何も届かず落ちた（`before-2-b4e32228-diagnostics.out`）。`changed_roots` で正準化し直すだけで前の向き先を外さない変異では、`only_in_real` が `real/Old.tz` の URI で残って落ちる（`mutant-2-no-retire.out`）。
   2 回目の `a_removed_or_repointed_root_link_is_resolved_again` の向け直しの場合は、通知が新しいディレクトリを予定に入れるようになったので、`settle` を 1 回にした（期待値は変えていない）。
3. **概要の指摘（`src/lsp.rs:3335`）: 古い取り消しを押し出すとき、受信のスレッドが先に読んだ同じ ID の取り消しの記録も消す。** 直した。
   受信のスレッドが cancel(A)、存在しない ID の取り消し 1024 個、cancel(A)、request(A) を、主ループが待ち行列へ移す前に読むと、1 つ目の A を押し出すときの `inbox.forget(A)` が、2 つ目の A の記録（集合なので 1 つ）も消す。2 つ目の A は `unmatched` に残るが集合にはなく、A を計算していた。
   1 つの並びだけを直すのでなく、`Inbox`（受信のスレッドの記録）と `Queue`（主ループが持つもの）の不変条件を決めて守るようにした。`Inbox` は ID ごとに、受信のスレッドが読んで主ループが手放していない取り消しの件数を持つ。主ループは、持つ取り消し（`unmatched` の 1 件と、待ち行列の要求の分の新しい `Queue::held`。要求が届くと `unmatched` の 1 件はそこへ移る）を手放すとき（届いたときに捨てる、押し出す、同じ ID の後のものに置き換える、それを持つ要求に答える）に、ちょうどその件数を `Inbox::release` で減らす。
   こうして ID ごとの件数は、まだ待ち行列へ来ていない取り消し、`unmatched` の 1 件、`held` の合計に等しく、片方から消すことで、もう片方がまだ要る記録を失うことがない。再記録（`Inbox::cancel`）と `Inbox::forget` はなくなった（逸脱 13）。この形で、「残る問題」にあった ID の使い回しの狭い競合も閉じた。
   テスト `src/lsp.rs` の `a_cancellation_read_again_outlives_its_pushed_out_copy`: 上の並びを、すべて記録してから待ち行列へ移しても、A の取り消しが残る。直す前は `assertion failed: inbox.is_cancelled("5")` で落ちた（`before-3.out`）。
   テスト `cancellations_agree_with_a_reference_model_in_any_interleaving`: 受信のスレッドの「記録する」「送る」と、主ループの「送られたものを待ち行列へ移す」「1 つ取る」「答える」の交互を、固定の種で無作為に 20,000 通り作る（メッセージは 1〜8 個、2 つの短い ID と `MAX_REMEMBERED_ID` を超える ID、覚える数は 1 か 2。`Queue::remembering` で注入する）。各要求の結果が参照のモデル `CancelModel`（取り消しは主ループだけが持ち、受信のスレッドが読んで待ち行列へまだ来ていない取り消しは、計算中の要求を取り消す）と同じこと、各段の後に `Inbox` の件数が「まだ来ていない分 + `unmatched` + `held`」に等しいこと、`unmatched` がモデルと同じことを確かめる。
   `b4e32228` の論理を写した変異は 146 通り目（この並びの族）で、集合の意味だけを戻した変異は 15 通り目（ID の使い回しの競合）で、どちらも結果の比較で落ちた（件数の検査を外して動かした。`mutant-3-b4e32228-logic.out`・`mutant-3-set-outcome-only.out`）。20,000 通りは約 0.2 秒。

文書: `docs/architecture.md` の言語サーバーの節（変更の通知でも根を正準化し直すこと、予定から外したディレクトリを同じ一覧で refresh しないこと、取り消しの件数と手放し方）。言語リファレンスは変えていない: `diagnostics.md` の「リンクの向き先を変えたときは、前の向き先の診断を消して新しい向き先を検査します」と `usage.md` の `lsp` の取り消しの記述は、この修正で正しくなるか正しいままで、取り消しの記録の仕組みは書いていない。`vsc/` は変えていない。

#### 3 回目の対応の後の確認（コードは `cd6d23c8`）

- T0: `cargo fmt --all -- --check` は差分なし、`cargo clippy --locked --all-targets -- -D warnings` は警告なし（`clippy.out`）。`cargo nextest run --locked --test lsp --lib -E 'binary(lsp) | test(lsp::) | test(fix::)'` は 111 passed（`tests/lsp.rs` は 72 件。指摘 1 と 2 で 2 件、`src/lsp.rs` の `mod tests` に指摘 3 で 2 件を足した。`t0-area.out`）。待ち行列・取り消し・根のテストの 21 件（2 回目の確認の 17 件の式に `test(a_root_dropped) | test(repointed_clean_root_link) | test(read_again) | test(reference_model)` を足したもの）を 6 回続けて動かし、6 回とも通った（`repeat.out`）。
- T1（`cargo nextest run --locked`）: 3379 passed・4 skipped（`t1.out`）。
- T2: `cargo build --locked` の後、`node tests/lsp_sessions.mjs target/debug/tsuzuri` は 4 つのセッションが成功（`sessions.out`）。`node scripts/check-links.mjs`（`--into _features/README.md` も）は 0 broken、`node scripts/check-guide-lite.mjs` は 88 references・0 failed、`git diff --check` は差分なし。言語リファレンスのページは変えていないので `check-docs` は動かしていない。
  `vsc/` は変えていないので、`npm run test:unit` などと拡張の統合テストは動かしていない。変わるのは、シンボリックリンクの根の向き先を変えたり消したりしたときと、取り消しの記録の数え方だけで、ふつうの診断の流れは変えない。`src/runtime/` は変えていないので `check-runtime-includes.sh` は動かしていない。段 13 の計測は取り直していない（解析の回数と、待ち行列の要求の扱いは変わらない）。`ci:full` は未実行。

#### 4 回目（`4a9fc47f` のコード）

GitHub Copilot の 4 回目のレビューの 1 件（2026-11-20）をコードで確かめた。本当の誤りで、直した。修正は、直す前に落ちる回帰テストと一緒にコミットした。記録は `target/scratch/g39/copilot4/`。

1. **4641120349（`src/lsp.rs:3312`）: 形の正しくない要求に答えないので、その ID の取り消しが残る。** 直した。
   `Queue::push` は、文字列か整数の ID を持つ text sync でないメッセージを、形（`jsonrpc` が `"2.0"`、`method` が文字列）を問わず要求として取る。`jsonrpc` のない要求、`jsonrpc` が `"1.0"` の要求、`method` のないメッセージ（応答の形）、`method` が文字列でない要求の前か後に同じ ID の取り消しが届くと、その取り消しはその要求の分（`Queue::held`）になる（前に届いたものは `unmatched` から移り、後のものは要求が待ち行列にある間に届けば加わる）。`serve` の -32600 の経路は `Queue::answer` を呼ばずに続けたので、`held` と `Inbox` の件数が、待ち行列が空になり数えたバイトが 0 に戻った後も残り（そうしたメッセージの ID 1 つにつき 1 つ、際限なく増える）、同じ ID の次の正しい要求を -32800 で答えていた。
   主ループは、取り出したメッセージの ID の鍵（`Queued::id_key`）を最初に取り、-32600 の経路でもそれに答える（`Queue::answer`）。こうして、文字列か整数の ID を持つメッセージは、どの経路でも答える（サーバーが終わる `exit`・入力の終わり・読み込みの失敗・出力の失敗を除く）。JSON でない本文（-32700）は ID を持たず、大きすぎるメッセージは読み込みの失敗で終わるので、待ち行列はどちらも要求として取らない。取り消された要求、-32801 の要求、`Session::handle` が拒む要求は、前から答えている。形の検査は `envelope_method` にまとめ、参照のモデルのテストと共有する。
   提案のもう 1 つの案（形の正しい要求だけを待ち行列に取る）は採らなかった: 待ち行列が要求として取るメッセージの規則（`Queue::push`、-32801 の判定、参照のモデル）を変え、拒んだメッセージの前に届いた取り消しが `unmatched` に残って、同じ ID の次の正しい要求を取り消す（件数は抑えられるが、この並びの結果は誤りのときと同じ）。答える案では、拒んだメッセージの ID を答えた ID として記録するので、その後に届く取り消しは遅れたものとして捨てる（仕様 13 の「答えた id」）。決定事項と既存のテストの期待値は変えていない。-32600 の応答の `id` は今までどおり `null` で、`jsonrpc` のない `$/cancelRequest` も今までどおり取り消しとして効く。
   テスト `src/lsp.rs` の `a_malformed_request_lets_go_of_its_cancellations`: `serve` に、上の 4 種の形の正しくない要求と、その前か後の同じ ID の取り消しと、同じ ID の正しい `workspace/symbol` を渡すと、4 つの -32600 と、正しい要求それぞれに -32002（`initialize` の前）が返る。直す前（`4a9fc47f`）は 4 つとも -32800 で落ちた（`before.out`）。-32600 の経路の `Queue::answer` だけを外した変異は、6 回とも同じく落ちた（`mutant-no-answer-in-serve.out`。受信のスレッドの時間に依らない）。
   テスト `cancellations_agree_with_a_reference_model_in_any_interleaving` を広げた: メッセージを 11 種（取り消し、`jsonrpc` のない取り消し、自分の ID を持つ取り消し（取り消しで、要求として計算する）、要求、`jsonrpc` のない要求・`method` のないメッセージ・`method` が数の要求（-32600 で拒んで答える）、ID を持つ text sync、通知、ID が `null` の要求、JSON でない本文）から作る。主ループの手順は `serve` と同じく、正しい要求は計算してから答え、ID を持つほかのメッセージはすぐ答える。各段の後に、前からの検査に加えて、`held` が待ち行列の要求と答えているメッセージの ID だけを持つこと、答えた ID の記録がモデルと同じことを確かめる。すべてに答えた後に、`held`・`requests` が空で、`Inbox` の件数が `unmatched` の ID に 1 つずつで、数えたバイトが 0 であることを確かめる。通り数は 20,000 から 40,000 にした（約 0.5 秒）。モデルは、ID を持つメッセージに答えると、その ID の `unmatched` の取り消しも手放す（ID を持つ text sync に対して `Queue::answer` が前から行うこと）。拒んだ要求に答えない変異（直す前の `serve` の手順）は、29 通り目で `held` の検査に落ちた（`mutant-unanswered-rejects.out`）。

文書: `docs/architecture.md` の言語サーバーの節（形の正しくないメッセージにも ID があれば答えること、参照のモデルのテストの範囲）。言語リファレンスは変えていない: `usage.md` の `lsp` の「取り消された要求（`$/cancelRequest`）は計算せずに `-32800` で」は、この修正で正しくなり、形の正しくないメッセージの扱いは書いていない。`vsc/` は変えていない。

#### 4 回目の対応の後の確認（コードは `ea645f9b`）

- T0: `cargo fmt --all -- --check` は差分なし、`cargo clippy --locked --all-targets -- -D warnings` は警告なし（`clippy.out`）。`cargo nextest run --locked --test lsp --lib -E 'binary(lsp) | test(lsp::) | test(fix::)'` は 112 passed（`tests/lsp.rs` は 72 件のまま、`src/lsp.rs` の `mod tests` に 1 件を足した。`t0-area.out`）。待ち行列・取り消し・根のテストの 22 件（3 回目の確認の 21 件の式に `test(a_malformed_request)` を足したもの）を 6 回続けて動かし、6 回とも通った（`repeat.out`）。
- T1（`cargo nextest run --locked`）: 3380 passed・4 skipped（`t1.out`）。
- T2: `cargo build --locked` の後、`node tests/lsp_sessions.mjs target/debug/tsuzuri` は 4 つのセッションが成功（`sessions.out`）。`node scripts/check-links.mjs`（`--into _features/README.md` も）は 0 broken、`node scripts/check-guide-lite.mjs` は 88 references・0 failed、`git diff --check` は差分なし。言語リファレンスのページは変えていないので `check-docs` は動かしていない。
  `vsc/` は変えていないので、`npm run test:unit` などと拡張の統合テストは動かしていない。変わるのは、形の正しくないメッセージの ID の取り消しの扱いだけで、ふつうの診断の流れは変えない。`src/runtime/` は変えていないので `check-runtime-includes.sh` は動かしていない。段 13 の計測は取り直していない（形の正しいメッセージの扱いは変わらない）。`ci:full` は未実行。

### PB08 との合流

PB08 が同時に `src/lsp.rs` を変える（`analyze` に CLI の `Parallelism` を渡す、`refresh` の publish の後へ `costly_sites`・`inferred_effects` を移す、所有権の事実の再利用）。このブランチの `refresh` の変更は、読み込みの失敗の経路（`load_error`・`retire`）、publish のループ（根の外の除外と `Sent`）、ループの後の `retire` の呼び出しだけで、`analyze` と `costly_sites`・`inferred_effects` の行は変えていない。ただし `refresh` の引数は根のディレクトリ（`&Path`）になり、`self.projects.insert(directory.to_owned(), …)` のように `directory` の扱いが変わったので、合流では PB08 の `refresh` の本文をこの形に合わせる。Copilot の 2 回目のレビューで、`refresh` の最初に `resolve_client_roots` と、どの根も指さなくなったディレクトリの `retire` を足した（解析の前で、`analyze` の行は変えていない）。3 回目のレビューで、その `retire` を `resolve_client_roots` の中へ移し、`serve` の期限の経路と `workspace/symbol` の refresh の繰り返しを `Session::refresh_due` にした（`refresh` の呼び方だけで、`refresh` の本文の解析の部分は変えていない）。4 回目のレビューで、`serve` の -32600 の経路が `Queue::answer` を呼ぶようにし、形の検査を `envelope_method`、ID の鍵を `Queued::id_key` にした（`refresh` は変えていない）。
送った URI の記録は `Sent`・`retire`・`load_error`・`changed_roots`・`uri`・`uri_under`、待ち行列は `Inbox`・`Queued`・`Queue`・`read_input`、取り消しの確認は `handle`・`code_actions`・`quick_fixes`・`fix_all` の引数 `cancelled` にまとめた。

### 更新した文書と変更したファイル

- 言語リファレンス: `_tsuzuri/language-reference/compiler/diagnostics.md`（LSP での表示）、`_tsuzuri/language-reference/compiler/usage.md`（`lsp`）。`docs/architecture.md`（言語サーバー）。`vsc/README.md`、`vsc/CHANGELOG.md`。`docs/language.md` と `README.md` は、言語サーバーの診断の寿命と待ち行列を書いておらず、誤りにならないので変えていない。
- `src/lsp.rs`、`src/fix.rs`、`src/mcp/check.rs`（`fix::run` の引数）、`tests/lsp.rs`、`tests/lsp_sessions.mjs`、`vsc/src/extension.ts`、`vsc/src/core.ts`（`notifiesServer`。レビューの指摘 5）、`vsc/src/test/extension.test.ts`、`vsc/src/test/core.test.ts`、`_features/G39-stale-editor-diagnostics.md`、`_features/README.md`。
