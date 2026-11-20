# G42: Windows x64 の VS Code で、テストの実行の結果が表示の直後に消えることがある（拡張の統合テストが Windows x64 でだけ時々落ちる）

| 項目 | 内容 |
| --- | --- |
| ID | G42 |
| 優先度 | P2（消えるのは早すぎる側で、直した後に古い診断が残る G39 の不具合ではない。ただし `vscode.yml` の Windows x64 の job を時々赤くし、リリースのタグの CI を止めうる） |
| 規模 | S〜M（原因の特定が先。拡張の `vsc/src/workflow.ts` と統合テスト） |
| 依存 | G39（done）, G40（done） |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-20（PR #120・#119 と、#119 を merge した後の main の `vscode.yml` の Windows x64 の job で 3 回観測した。原因は確かめていない） |
| 承認 | 不要（拡張の内部の判定とテストの安定化。利用者に見える振る舞いを変えるときは G39 の仕様 3 の範囲で行う） |
| 改善する劣位 | 不具合（推定）: Windows x64 で、テストの実行・Debug Project の結果（source `Tsuzuri`）が、プロジェクトが変わっていないのに表示の直後に消えるか、表示されない。CI: 統合テストの G39 Phase 1 の段が Windows x64 でだけ時々落ちる |
| 手本にする既存実装 | `vsc/src/workflow.ts`（`rootState`・`clear`・`written`・`changedOutside`・`stamped`・`publishDiagnostics` の判定）、`vsc/src/core.ts` の `affectsRoot`、`vsc/src/test/extension.test.ts` の G39 Phase 1 の段 |
| 主な影響ファイル | `vsc/src/workflow.ts`, `vsc/src/test/extension.test.ts`, `vsc/README.md`（振る舞いを変えるとき）, `vsc/CHANGELOG.md` |

## 観測

どれも `vscode.yml` の `package (windows-2022, win32-x64)` の job で、ほかの 4 環境（`darwin-arm64`・`linux-arm64`・`linux-x64`・`win32-arm64`）は同じコミットで通った。同じ tree が Windows x64 でも通ることがある。

| run | 対象 | 段 | 落ちた検査 |
| --- | --- | --- | --- |
| 44720310026（1 回目） | PR #120（G40）の head | `npm run test:installed` の「テストの実行が言語サーバーの警告の写しを出さない」 | テストの実行の直後、根の `Main.tz` の `E2006`（失敗したテストの結果）がない。job の再実行は通った |
| 44756268093（1 回目） | main `8897f24b`（#119 の merge。tree は PR #119 の head `b3f5ab7e` と同じで、その PR の run 44747760169 は通った） | `npm test` の「path 依存の結果が根の外の変化で消え…」 | `extension.test.js:507`: `0 !== 1`。根の外の `packages/lib/Calc.tz` の `W1001` の結果の写しがない |
| 44756268093（2 回目） | 同じ | 同じ段 | `extension.test.js:462`: `0 !== 1`。同じ段の、もっと前の `Calc.tz` の結果の検査 |

3 回とも「実行の結果が、出るべきときに出ていない」側で、残るべきでない結果が残った失敗はない。

## 結果を消す・出さない判定（`vsc/src/workflow.ts`）

- 根の下の削除（`affectsRoot` が残すもの。`.` で始まる名前と `target/tz/kg/` 以外）は、時刻に関係なく根の結果を消す。
- 根の下のソースの作成・書き込みは、そのファイルの時刻（`stat` の `max(mtimeMs, ctimeMs)`。`stat` に失敗したら `Date.now()`）が結果の表示より後なら消す。根の外なら `writtenOutside` に記録し、根の外を指す結果を消す。
- どの根にも入らない削除は、根の外を指す結果を消す（`changedOutside`）。
- 実行の結果は、実行の始まり（`saveAll` の直後）の後に変更の番号が進んでいるか、ファイルの時刻が始まりより後なら出さない。

## 仮説（どれも確かめていない）

1. Windows の `ctimeMs`（NTFS の ChangeTime）が、書き込みより後に変わる。ウイルス対策の走査や属性の変更が考えられる。遅れて届いた変更の事象の `stat` の時刻が、実行の始まりや結果の表示より後になり、結果を出さないか消す。
2. 監視の事象の遅れ。前の段の書き込みや保存の事象が、次の実行の始まりの後に届く。時刻の比較は 1 と同じく `ctime` に左右される。
3. compiler の知識グラフの差し替え（`target/tz/kg` を `.tsuzuri-<pid>-<id>/previous-N` へ移す `rename`）か、ほかの一時的なファイルが、`affectsRoot` が残すパスの削除として届く。

## 進め方

1. 判定の記録: 拡張の出力に、結果を消す・出さない判定ごとに、事象の種類、パス、ファイルの時刻、比べた時刻、理由を書く。設定か環境変数で有効にする。統合テストでは有効にして、落ちたときに job のログへ出す。
2. 再現: Windows x64 の host（`vscode.yml` の `workflow_dispatch` か、手元の Windows）で該当の段を繰り返し、1 の記録から、どの判定が結果を消したかを特定する。
3. 直し: 特定した原因に合わせて直す（例: Windows では `mtime` だけで比べる、`stat` の時刻が実行の始まりより前の事象は数えない、差し替えの一時的なパスを除く）。G39 の仕様 3（プロジェクトが変わったら消す）を弱めない。回帰テストは、可能なら事象の列を差し替えた単体テストで書く。

## 受け入れ条件

- [ ] 結果を消した判定の記録が取れ、3 回の観測の原因を特定している（できなければ、特定できなかった理由と候補を記録している）。
- [ ] 直した後、Windows x64 の `vscode.yml` で、G39 Phase 1 の段を含む統合テストが続けて 10 回通る。
- [ ] G39 Phase 1 の統合テストの検査（変えたら消える、変えていなければ残る）がすべての環境で通る。
- [ ] 振る舞いを変えたときは `vsc/README.md`・`vsc/CHANGELOG.md` を更新している。

## 対象外

- 結果を消す条件そのものの見直し（G39 の仕様 3）。早く消すことは安全側で、このチケットは「変わっていないのに消える」ものだけを扱う。
