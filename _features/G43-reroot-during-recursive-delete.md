# G43: VS Code で、開いているファイルのあるプロジェクトのフォルダーを中身から順に消すと、その上のフォルダーが Testing ビューの根として残ることがある

| 項目 | 内容 |
| --- | --- |
| ID | G43 |
| 優先度 | P3（`rm -r` のように中身から順に消す間の短い時間にだけ起きる。Explorer の削除はごみ箱への移動で、フォルダーごとの 1 つの事象になる。残るのは空か無関係なテストの根で、**Refresh Tests** や次の決め直しでは消えない） |
| 規模 | S（拡張の `vsc/src/testing.ts`・`vsc/src/workflow.ts` と統合テスト） |
| 依存 | G40（done） |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-20（main `929eacb8`（0.1.9 のリリースの merge）の `vscode.yml` の `package (macos-15, darwin-arm64)` で、G40 の統合テストの検査「消したフォルダーの上のフォルダーにテストの根がない」が落ちた（run 44784514826）。同じコードはタグ `vsc-v0.1.9` の run 44777410811 と PR #120 の CI で 5 環境とも通った） |
| 承認 | 不要（G40 の仕様 5 の範囲の直し。根を外す規則を変えるときは G40 の D4 との関係を記録する） |
| 改善する劣位 | 不具合: 開いているファイルのあるプロジェクトのフォルダー（`loose/extra/demo`、`Main.tz` を持つ）を中身から順に消すと、その上の印のないフォルダー（`loose/extra`）がテストの根として残る。CI: G40 の統合テストの検査が時々落ちる |
| 手本にする既存実装 | `vsc/src/testing.ts` の `usedRoots`・`reroot`・`documentRoot`、`vsc/src/workflow.ts` の `documentHasProject`・`projectRootFor`、`vsc/src/core.ts` の `ProjectRoots.settled` |
| 主な影響ファイル | `vsc/src/testing.ts`, `vsc/src/workflow.ts`, `vsc/src/test/reroot.test.ts`, `vsc/src/test/extension.test.ts` |

## 観測

run 44784514826 の macOS の job の統合テストは、`loose/extra/demo/Main.tz`（開いた文書）と `loose/gone/Other.tz` を作り、2 つをテストの根にしてから、`rm(demo, { recursive: true })` と `rm(gone, …)` で消した。`demo` と `gone` のクライアントとテストの根が消えるのを待った後の `assert.ok(!api.testing.roots.has(extra))` で、`no test root for the folder above the deleted folder` と落ちた（`vsc/src/test/extension.test.ts` の G40 の段）。

## 推定される原因（確かめていない）

`rm -r` は中身を先に消し、フォルダーを最後に消す。その間、`demo` のフォルダーはあるが `demo/Main.tz`（印）はない。

1. その間に決め直しか文書の事象が来ると、開いている `demo/Main.tz` の根を探す。`projectRootFor` は印のない `demo` から上へ探し、`loose` が App を含むので、`extra` を根にする。
2. `documentHasProject` は文書のフォルダー（`demo`）があるかだけを見るので、まだ消えていない `demo` を見て「プロジェクトがある」とする。`usedRoots` か開いた文書の処理が `extra` をテストの根に足す。
3. `demo` のフォルダーが消えた後の決め直しでは、`extra` は使われていないが、フォルダーがあり、`directoryRoot(extra).root === extra`（印のないフォルダーの根として成り立つ）。このため、G40 の仕様 5（作り直した集合になく、根として成り立たなくなったものだけを外す）では外れない。

同じ間に言語サーバーのクライアントも `extra` に起動しうる（`open` も `documentHasProject` を使う）。検査は `!api.clients.has(extra)` も確かめているが、こちらは落ちていない。

## 進め方

1. 再現: `reroot.test.ts` の差し替えたファイル システムで、文書の印のファイルが消え、フォルダーが残っている状態を作る。そこで決め直しと開いた文書の処理を動かし、`extra` が根になることを確かめる。
2. 直し（案）:
   - 開いている文書の根を数えるとき、文書のファイル自体がなければ数えない（`documentHasProject` に文書の存在の確認を足す）。消されている途中のファイルの文書は、根を作らない。
   - あるいは、開いた文書だけが足した根を記録し、その文書の根でなくなったら外す。
   - どちらにするかは、G40 の D4・仕様 5（文書を閉じたプロジェクトの根を残す）を崩さないほうを選ぶ。
3. クライアントの側（`open`）も同じ条件にそろえる。
4. 回帰テスト（単体と、統合テストの該当の検査）。

## 受け入れ条件

- [ ] 消している途中のフォルダーの文書が、上のフォルダーをテストの根にも言語サーバーの根にもしない（単体テストで再現し、直した後に通る）。
- [ ] G40 の統合テストの検査が、5 環境の `vscode.yml` で続けて通る。
- [ ] G40 の仕様 5 の振る舞い（文書を閉じたプロジェクトの根が、無関係なファイルの作成で消えない）が変わらない。
- [ ] 振る舞いを変えたときは `vsc/Development.md`・`vsc/CHANGELOG.md` を更新している。

## 対象外

- ごみ箱への移動や、Explorer の削除のような 1 つの事象の削除（G40 で扱い済み）。
