# G40: VS Code でプロジェクトでないフォルダーを開くと、その根のクライアントが中のパッケージのファイルまで受け持つ（消えない偽のエラー、診断・ホバーの重複、このリポジトリでの `E1017`）

| 項目 | 内容 |
| --- | --- |
| ID | G40 |
| 優先度 | P2（直下に `Tsuzuri.toml` も `Main.tz` もないフォルダーの下にパッケージがあり、パッケージの外の `.tz` を開いたときに起きる。このリポジトリを開いて `std/`・`tests/` の `.tz` を開くと必ず起きる） |
| 規模 | M（拡張だけ。根の決め方、残る場合の selector と診断の絞り込み、印の変化への追従、統合テスト） |
| 依存 | G14, G16（すべて done）, (G39) |
| 後続 | – |
| 状態 | done（段 1〜5。2026-11-20。テストの根は、作り直した集合になく、根として成り立たなくなったものだけを外す（仕様 5 を実装に合わせて改めた。利用者の委任による判断）。「実装と検証」を参照） |
| 起票 | 2026-11-19（G39 の調査で見つけ、利用者の依頼で別のチケットにした。HEAD `af644794`（拡張 0.1.7）で、VS Code 1.103.2 に拡張を入れ、言語サーバーの各プロセスが受け取ったメッセージを記録する compiler の shim（`tsuzuri.compilerPath`）で再現した） |
| 承認 | D1 は 2026-11-19 に利用者が承認した（「承認が必要なものについては、あなたが考えるベストな方法であれば全て承認します」。D1 は起票者が拡張だけの案のうち最善と判断した案のまま）。D1 は印のないファイルの根の決め方を変える。言語と CLI は変えないが、そうしたファイルのモジュール名と、ディレクトリをまたぐ参照の解決が変わる。D2〜D6 は既定案 |
| 改善する劣位 | 不具合: (1) フォルダーの根のクライアントが、中のパッケージのファイルにマニフェストを読まない解析で偽のエラーを出し、直しても消えない。(2) 同じ診断・ホバー・シンボル・補完が 2 つずつ出る。(3) 根の外の `.tz`（メモ、試し書き）のホバー・定義・シンボル・色分けが、中のパッケージの解析の失敗のせいで何も返さない。(4) このリポジトリでは根のクライアントの読み込みが毎回 `E1017` で失敗し、それがディレクトリの URI に付く。テストの発見も、ビルドできないフォルダーの根を対象にする（タスクがワークスペースのフォルダーの根にしか出ないことは対象外） |
| 手本にする既存実装 | `vsc/src/core.ts` の `projectRoot`（`fs` で上へ印を探す。単体テストは `vsc/src/test/core.test.ts` の `project root respects nested projects and source subdirectories`）、`vsc/src/extension.ts` の `open`（根ごとの `LanguageClient`、documentSelector の作り方と escape）、`vsc/scripts/test-extension.mjs`（統合テストのワークスペースの作り方）、vscode-languageclient の `middleware.handleDiagnostics` |
| 主な影響ファイル | `vsc/src/core.ts`, `vsc/src/extension.ts`, `vsc/src/workflow.ts`, `vsc/src/testing.ts`, `vsc/src/test/core.test.ts`, `vsc/src/test/extension.test.ts`, `vsc/scripts/test-extension.mjs`, `vsc/README.md`, `vsc/Development.md`, `vsc/CHANGELOG.md`, `docs/architecture.md`, `_tsuzuri/language-reference/compiler/usage.md`, `_features/README.md` |

## 目的

1 つの `.tz` ファイルを、1 つのクライアント（言語サーバーのプロセス）だけが受け持つようにする。
今は、プロジェクトでないフォルダーの根のクライアントの documentSelector（`<根>/**/*.{tz,tt,tc}`）が、中のパッケージのファイルにも当たる。
そのため、パッケージのファイルを 2 つの言語サーバーが別々の根で解析し、片方はマニフェストを読まないので偽のエラーを出す。

1. 印（`Tsuzuri.toml` か `Main.tz`）のないファイルの根を、中に別のプロジェクトを含まないディレクトリにする（D1）。
2. それでも根の下にプロジェクトが残る場合（印のないファイルが、パッケージを含むディレクトリの直下にある）は、そのクライアントが直下のファイルだけを受け持つ（D2）。
3. 印が増えたり消えたりしたら、根を決め直す（D4）。

## 着手条件と停止条件

### 着手条件

- G14・G16 が done であること。G39 の Phase 1（D1: テストの発見が診断を出さない）が done であること（下の落とし穴を参照）。
- D1 は承認済み（2026-11-19。メタ情報の「承認」）。
- 下の「現状」の再現 (a) を、手元の VS Code か統合テストと同じ方法で確かめていること。`vsc/` の統合テストは、`ELECTRON_RUN_AS_NODE` と `VSCODE_*` の環境変数を外して動かす。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- 言語サーバー（`src/lsp.rs`）か、モジュールの発見（`src/driver.rs` の `collect_sources`）を変えないと直せないと分かった（このチケットは拡張だけを変える。下の対象外を参照）。
- プロジェクトの中のプロジェクト（再現 (c)・(c2)）の振る舞いが変わる。
- 既存の `vsc/src/test/core.test.ts`・`vsc/src/test/extension.test.ts` の期待値を、このチケットの決定事項にない理由で変えないと通らない。

## 現状（2026-11-19、`main` の `af644794` で確認）

### 根の決め方

- `projectRoot`（`vsc/src/core.ts:33-50`）は、ファイルのディレクトリから上へ `Tsuzuri.toml` か `Main.tz` を探し、ワークスペースのフォルダーで止まる。見つからなければ `fallback = workspaceRoot ?? directory`（`:40`）で、ワークスペースのフォルダーを根にする。
- `projectFor`（`vsc/src/workflow.ts:7-21`）は、`tsuzuri.projectPath` があればそれを、なければ `projectRoot` を返す。
- `open`（`vsc/src/extension.ts:26-52`）は根ごとに `LanguageClient` を 1 つ起動する。documentSelector は `<根>/**/*.{tz,tt,tc}`（`:42`）、`workspaceFolder` は根（`:43`）、watcher は根の `**/*.{tz,tt,tc,toml}`（`:37`）。クライアントは再起動のコマンドか終了でしか止まらない（`:80-84`・`:91-96`）。
- selector が重なると、vscode-languageclient 9.0.1 はどちらのクライアントにも `didOpen`・`didChange`・`didClose` と要求を送る（`vsc/node_modules/vscode-languageclient/lib/common/features.js:117-127`、`textSynchronization.js:38-53`・`178`）。
- 言語サーバーの `Session::project_root`（`src/lsp.rs:319-326`）は、クライアントの根のうち最も長い接頭辞を選ぶだけで、印を探さない。
- マニフェストのないフォルダーの読み込みは、`.` で始まる名前と知識グラフのディレクトリ以外をすべて再帰的に読み（`src/driver.rs:1758-1844`）、中の `Tsuzuri.toml` を無視して、そのファイルをフォルダーのモジュールにする。
  CLI も同じで、`tsuzuri check <ファイル>` は親のディレクトリを根にして再帰的に読む（`_tsuzuri/language-reference/organizing-tsuzuri/modules.md:144-148`）。
- テストの発見（`vsc/src/testing.ts`）は、開いた文書ごとに `projectFor` の根を足し（`:93-95`・`:342-346`）、`tsuzuri test <根> --list --json` を動かす。タスクはワークスペースのフォルダーごとに `projectFor(folder)` の 6 つだけを出す（`vsc/src/workflow.ts:125-135`）。

### 再現

確認済み。どれも別の担当が自分の harness で再現し直している。harness と記録は `target/scratch/g40-pb08/g40/` と `target/scratch/g40-pb08/verify-g40-routing/` にある（リポジトリには置かない）。

**(a) プロジェクトでないフォルダーの下のパッケージ**: ワークスペースのフォルダー `ws` の直下に印がなく、`ws/app/`（`Tsuzuri.toml`・`Main.tz`・ヘッダーのない `Util.tz`）と `ws/scratch/Try.tz` がある。

1. `scratch/Try.tz` を開くと、根 `ws` のクライアントが起動する。`app/Main.tz` を開く前から、そのサーバーが `app/Main.tz` に偽の `E1002`（`unknown value 'App::Util'`）を出す。マニフェストを読まないので、ヘッダーのない `Util.tz` は `App::Util` にならない。
2. `app/Main.tz` を開くと根 `ws/app` のクライアントも起動し、`app/Main.tz` の `didOpen`・`didChange` とすべての要求が 2 つのサーバーへ届く。
3. `app/Main.tz` に入れた本物のエラーは 2 件ずつ出て、直すと両方消える。偽の `E1002` は、開いている間ずっと残る。
4. `scratch/Try.tz` では、ホバー・定義・参照・シンボル・色分けがすべて何も返さない。根 `ws` の解析が `app` の偽のエラーで毎回失敗し、索引を作れないため（`src/lsp.rs:1518-1553`）。
5. テストの根は `ws` と `ws/app` になり、`tsuzuri test ws --list --json` が同じ偽の `E1002` で失敗する。タスクは `ws` の 6 つだけで、`tsuzuri check ws` は `E2001`（`Main.tz` がない）。ビルドできる `app` のタスクは出ない。

```text
SNAPSHOT A.0 ...: ws/app/Main.tz [tsuzuri] E1002 @2:34
SNAPSHOT A.3 real error removed: ws/app/Main.tz [Tsuzuri] E1002 | ws/app/Main.tz [tsuzuri] E1002
SERVER ...cwd=<B>/ws text sync: didOpen <B>/ws/scratch/Try.tz x1; didOpen <B>/ws/app/Main.tz x1; didChange <B>/ws/app/Main.tz x2
PROVIDERS A.1 ws/scratch/Try.tz ... {"hover":"0 []","definition":"0 []",..."symbols":"null []"...}
```

（`target/scratch/g40-pb08/g40/e2e/runs/a/events.log`・`analysis.txt`・`shim/invocations.log`）
`Util.tz` にヘッダー（`namespace App`）があって両方のサーバーが同じ答えを出すときも、ホバーとシンボルが 2 つずつ返る（`runs/a2`: `hover 2`、`symbols 2 [main, main]`、補完 337 件のうち 184 件のラベルが重複）。

**(b) このリポジトリを開く**: `examples/point/Main.tz` を開くと根 `examples/point` のクライアント、`std/Array.tz` を開くとリポジトリの根のクライアントが起動する。

- リポジトリの根のサーバーにも `examples/point/Main.tz` の `didOpen` が届く（起動時に開いている文書を送る経路）。
- その読み込みは毎回 `E1017`（`module discovery exceeds 1024 directories or 16 path segments`）で失敗し、それが `vsc/toolchain/zig/lib/libc/include/generic-glibc/scsi` のディレクトリの URI に付く。テストの発見も同じ `E1017` を `tsuzuri-build` に入れる。どのディレクトリに付くかは、走査が上限に達した順序で決まる。
- `std/Array.tz` のホバー・定義・シンボルは何も返さない。タスクはリポジトリの根の 6 つだけ。
- 印のあるディレクトリを飛ばしても、根の下のディレクトリは約 1 万あり（`target/` を含む）、上限の 1024 を超える。

（`target/scratch/g40-pb08/g40/e2e/runs/b/`、`verify-g40-routing/e2e/runs/b/`）

**(c) プロジェクトの中の path 依存**: `app/lib` が自分の `Tsuzuri.toml` を持つ path 依存のとき、両方のクライアントが `lib` のファイルに同じ診断を出し、直すと一緒に消える。
`app` のサーバーは `lib` の未保存のバッファを使って解析している（`lib` のバッファを渡さないと、バッファにしかない関数の呼び出しが `E1002` になる。`target/scratch/g40-pb08/g40/proto/logs/overlay.out`）。

**(c2) 依存でない入れ子のパッケージ**: `app/tools` が自分の `Tsuzuri.toml` を持ち、`app` の依存でないとき、`app` のクライアントが `tools` のファイルに出す `E1002` は、`tsuzuri check app` が出すものと同じで、偽ではない。

**(d) 2 つのパッケージを別のワークスペースのフォルダーにする**: 重ならない（対照）。

### 原因

フォルダーの根（印のないファイルのための根）が、その下にある別のプロジェクトのファイルも selector で拾い、サーバーはマニフェストを読まないフォルダーとしてそれを読み込む。
VS Code の glob には「この下を除く」がなく（`vsc/node_modules/@types/vscode/index.d.ts` の glob の説明は文字単位の `[!...]` だけ）、DocumentFilter にも除外がない。

### 関連する観測（このチケットの対象外）

- `tsuzuri.projectPath` を設定すると、その外の印のないファイルは解析されない（再現 (e)。今の仕様どおり）。
- 1 つの言語サーバーのプロセスは、何もしていないときも約 78 MB を使い、クライアントは止まらない（`target/scratch/g40-pb08/g40/proto/logs/rss.out`）。

## 仕様（直した後の振る舞い）

1. ファイルの根は次の順で決める（D1）。
   1. `tsuzuri.projectPath` があれば、それ（今と同じ）。
   2. ファイルのディレクトリから上へ、ワークスペースのフォルダーまでに `Tsuzuri.toml` か `Main.tz` があれば、最も近いそのディレクトリ（今と同じ）。
   3. なければ、ファイルのディレクトリから上へ、ワークスペースのフォルダーを超えない範囲で、**下に印を 1 つも含まない最も上のディレクトリ**。
      印を含むかどうかは、モジュールの発見（`src/driver.rs:1758-1844`）と同じ規則で数える。`.` で始まる名前と知識グラフのディレクトリ（`.tsuzuri-kg` を持つもの。`:1810-1813`）は辿らない。
      根を含めて 1024 個を超えるディレクトリ、根から 16 段下のディレクトリ（`depth >= 16`、`:1767`）、4096 個を超えるソース（`:1827`）、ソースかディレクトリのシンボリックリンク（壊れたリンクを含む。`:1795-1808`、`modules.md:144`）のどれかに当たったら「含む」とみなす（その根の読み込みは `E1017` か `E1011` で失敗するため）。
   4. ファイルのディレクトリ自体が下に印を含むなら（印のないファイルが、パッケージを含むディレクトリの直下にある）、そのディレクトリ。これを「残る根」と呼ぶ。残る根は 4 で決まった根だけで、印のある根（2）と `tsuzuri.projectPath` の根（1）は、下に別のパッケージがあっても残る根にしない。
   どのワークスペースのフォルダーにも入らないファイルは、今と同じく上へファイルシステムの根まで印を探し、なければそのディレクトリ。そのディレクトリが下に印を含むなら残る根として扱う（仕様 2）。
2. 残る根のクライアントは、根の直下のファイルだけを受け持つ。documentSelector は `<根>/*.{tz,tt,tc}`（`**` でなく `*`）にする（D2）。
   そのサーバーは根を再帰的に読むので、直下でないファイルの診断を送ってくる。`middleware.handleDiagnostics` で根の直下でない URI を空にし、ワークスペースのシンボルの結果からも外す。
3. 印を含まない根のクライアントは、今と同じく `<根>/**/*.{tz,tt,tc}` を受け持つ。下に印がないので、ほかのクライアントと重ならない。
4. テストの根とタスクの `project` も、1 の根に従う。
5. ワークスペースのフォルダーの下でファイルかディレクトリが作られるか消えたら（`Tsuzuri.toml`・`Main.tz` に限らない。ソースの作成・削除は 4096 個の上限の判定を、ディレクトリの作成・削除は 1024 個の上限の判定を変える。フォルダーの作成・削除・移動を含む。設計の watcher を参照）、1 の結果の cache を捨て、次をする（D4）。
   今あるクライアントの根 R ごとに、R の直下の仮のファイルの根と、それが残る根かどうかを決め直し、根が R でなくなったか、残る根かどうか（selector の `*` と `**`）が変わったクライアントを止める（止めないと、古い selector がほかのクライアントのファイルを受け持ち続ける）。
   開いている文書ごとに `open` を呼んで足りないクライアントを起動し、テストの根を `refreshRoots` と同じ規則で作り直して、その集合になく、かつ根として成り立たなくなった根（直下の仮のファイルの根がその根でなくなった根、根のフォルダーがなくなった根。クライアントの D4 と同じ規則）を Testing ビューから外す。
   文書を閉じたプロジェクトの根は、無関係なファイルの作成では外さない（2026-11-20、実装のレビューの指摘 5 を受けて改めた。下の「実装と検証」）。
6. 次は変えない: 印のあるプロジェクトの根、プロジェクトの中のプロジェクト（再現 (c)・(c2)）、`tsuzuri.projectPath`、マルチルートのワークスペース、言語サーバーとモジュールの発見。

### 変わること（利用者に見える）

- 再現 (a): クライアントは `ws/scratch` と `ws/app` になる。偽の `E1002` と重複が消え、`scratch/Try.tz` のホバーが返る。テストの根に `ws` が入らない。
- 再現 (b): クライアントは `examples/point`・`std`・`tests/conformance/ABI` などになる。ディレクトリの URI の `E1017` は出なくなる。
  代わりに、`std/` を根にした解析は std のモジュール名を予約名として `E1011` を 50 件出し、`tests/conformance/ABI` の fixture は 11 件の `E1011` を出す（`target/scratch/g40-pb08/g40/proto/logs/roots.out`。std の開発を言語サーバーで扱う仕組みは対象外）。
- パッケージを含むフォルダーの下で、印のないファイルがディレクトリをまたいで参照し合っていると（`x/X.tz` が `Root::y::Y.f()` を呼ぶ）、今は解決するが、直した後は `E1002` になる。
  2 つのディレクトリが別の根になるため（`verify-g40-routing/e2e/runs/xdir` と `xdir3`）。`tsuzuri check x/X.tz` も同じ `E1002` を出す。パッケージを含まず、モジュールの発見の上限にも収まるフォルダーでは、今と同じく 1 つの根なので変わらない。
- そうしたファイルのモジュール名は、根からの相対パスで決まるので変わる（`Ws::scratch::Try` が `Scratch::Try` になる）。
- 印を含まなくても、根を含めて 1024 個を超えるディレクトリか 16 段下のディレクトリを持つフォルダー（`node_modules/`・`target/` を含むものなど）は、今は読み込みが毎回 `E1017` で失敗するが、直した後はファイルに近い、上限に収まるディレクトリを根にする。そこではモジュール名とディレクトリをまたぐ参照が変わる。
- 残る根（パッケージと同じフォルダーの直下にある印のないファイル）は、今と同じくパッケージをマニフェストなしで読む。パッケージのファイルがその読み方で解析できないと（ヘッダーのない `Util.tz` など）、そのファイルのホバー・定義・シンボルは今と同じく何も返さず、そのフォルダーのテストの根（`vsc/src/testing.ts:89-90`）も同じ偽の `E1002` で発見に失敗する（`tsuzuri check loose/Notes.tz`・`tsuzuri test loose --list --json` も同じ。`verify-g40-routing/cli/cli.out`、`g40/proto/logs/roots.out` の `iii-residual-top-level`）。
  改善する劣位の (3) は、パッケージの外のサブディレクトリにあるファイルでだけ直る。

## 設計

| ファイル | 関数・場所 | 変更 |
| --- | --- | --- |
| `vsc/src/core.ts` | `projectRoot`（33〜50 行） | 仕様 1 の 3・4 を足す。ディレクトリごとに「下に印を含むか」を幅優先で調べ（印を見つけたらそこで止め、D3 の条件で打ち切って「含む」とする）、結果をディレクトリごとに cache する。走査の関数は差し替えられる形にし、純粋な部分は単体テストで確かめる |
| `vsc/src/core.ts` | 新しい関数 | 根が「残る根」かどうかの判定（仕様 1 の 4 で決まった根だけ。根自体に `Tsuzuri.toml` も `Main.tz` もなく、サブディレクトリに印があるか D3 の打ち切りに当たる）と、パスが根の直下かどうかの判定。印のある根（再現 (c)・(c2) のように下に別のパッケージがあっても）と `tsuzuri.projectPath` の根は残る根にしない |
| `vsc/src/extension.ts` | `open`（26〜52 行） | 残る根には documentSelector `<根>/*.{tz,tt,tc}` と、直下でない URI を空にする `middleware.handleDiagnostics`、`middleware.provideWorkspaceSymbols` の絞り込みを付ける。今の escape（`:42`）をそのまま使う |
| `vsc/src/extension.ts` | 新しい watcher | 文字列の glob（`vsc/src/testing.ts:112` と同じく、すべてのワークスペースのフォルダーを覆い、フォルダーの追加にも追従する）で、`**` の作成・削除をすべて（名前に関わらず。印、ソース、ディレクトリ）見て cache を捨てる。印だけでなくソースの作成・削除（4096 個の上限）とディレクトリの作成・削除（1024 個の上限）も D3 の判定を変えるので、`.tz`・`.tt`・`.tc` の事象も除かない。
VS Code はフォルダーの削除・移動を親の 1 件にまとめる（`vsc/node_modules/@types/vscode/index.d.ts:13999-14005`、G39 の Phase 1 の `**` の削除の watcher と同じ理由）ので、印の glob だけではパッケージのフォルダーを消すか移したときに cache が古いまま残る。内容の変更の事象は判定を変えないので見ない。
事象は続けて届く（`git checkout` など）ので、`vsc/src/testing.ts:98-111` と同じく 300 ms まとめてから決め直す。`onDidChangeWorkspaceFolders` でも cache を捨てる。cache を捨ててから、クライアントの決め直し（仕様 5）とテストの根の作り直しを同じ処理の中で順に呼ぶ（`testing.ts` の `changed` が先に動くと古い根を再発見する）。根か selector の種類が変わるクライアントだけを止めて起動し直す（今の再起動のコマンドは全部を止める） |
| `vsc/src/testing.ts` | `refreshRoots`（83〜96 行）と新しい購読 | 印の変化のとき、`refreshRoots` と同じ規則（ワークスペースのフォルダーごとの根（`:84-92`）と開いている文書の根（`:93-95`））で根の集合を作り直し、その集合にない根を外す（今は外さない。`:115-124` はワークスペースのフォルダーの削除だけ）。開いている文書だけで判定すると、文書を開いていないフォルダーの根まで消える |
| `vsc/src/workflow.ts` | `projectFor`（7〜21 行） | `projectRoot` の cache を使う。`projectFor` は watcher の事象ごと（`vsc/src/testing.ts:98-111`）、`refreshRoots` の開いた文書ごと、コマンドとタスクから呼ばれるので、毎回走査しない |

`projectRoot` の走査は、このリポジトリでは 1 回に約 1025 回の `readdir`（66〜126 ms）になる（`target/scratch/g40-pb08/verify-g40-routing/proto/hma.out`。深さ優先だと `vsc/toolchain` で上限に達する）。
幅優先で印を先に見つければ早く止まり、cache で 2 回目からは走査しない。

## 実装手順

各段で、先にテストを書き、HEAD で落ちることを確かめてから直す。

### 段 1: 根の決め方

- 変更: `vsc/src/core.ts` の `projectRoot` と cache。
- 確認: `vsc/src/test/core.test.ts` の `project root respects nested projects and source subdirectories` に、`root/app/Main.tz` を残したまま次を足す。
  - `projectRoot(<root>/scratch/Try.tz, root)` が `<root>/scratch`（HEAD は `root` なので落ちる）。
  - `projectRoot(<root>/Library.tz, root)` は `root` のまま（残る根）。既存の検査（`core.test.ts:148-149`）がそのまま通ることを確かめる。
  - 印のない別の一時ディレクトリで、`projectRoot(<r>/a/X.tz, r)` は `r` のまま（パッケージを含まないフォルダーは変わらない。HEAD でも通る）。
  - 1024 個を超える印のないディレクトリの木、ソースかディレクトリのシンボリックリンクを含む木は「含む」とみなす。存在しないディレクトリの未保存のファイル（`readdir` が `ENOENT`）でも落ちない。
  - `root/app/lib/Tsuzuri.toml` を足しても、根 `root/app` は残る根と判定されない。
  - `workspaceRoot` なしで `projectRoot(<r>/Notes.tz)`（`<r>/app/Main.tz` あり）が `r` で、残る根と判定される。
  - 同じディレクトリの 2 回目の呼び出しが `readdir` を呼ばない（走査の関数を差し替えて回数を数える）。cache を捨てた後は、足した印が結果に反映される。
  - ソースの上限を両方向にまたぐ: 差し替えた走査で、印のない木のソースを 4096 個から 4097 個にして cache を捨てると根が分かれ、4096 個に戻して cache を捨てると 1 つの根に戻る。ソースの作成・削除の事象で cache が捨てられることも確かめる（仕様 5）。
  - `cd vsc && npm run test:unit`。

### 段 2: 残る根の selector と診断の絞り込み

- 変更: `vsc/src/extension.ts` の `open`。
- 確認: 統合テスト（段 4）の 3。

### 段 3: 印の変化への追従

- 変更: `vsc/src/extension.ts`、`vsc/src/testing.ts`、`vsc/src/workflow.ts`。
- 確認: 統合テスト（段 4 の 3 の後）で、`<loose>/Main.tz`（`def main :: unit -> i32 = \() -> 0`）を作ると、`scratch/Try.tz` と `Notes.tz` の根が `<loose>` になる。
  `api.clients.has('<loose>/scratch')` が偽になり（止まる）、`<loose>` のクライアントは selector が `*` から `**` になるので別のオブジェクトとして起動し直し、`api.testing.roots.has('<loose>/scratch')` が偽になる。
  `<loose>/Main.tz` を消すと、`<loose>/scratch` のクライアントとテストの根が戻る。HEAD では最初から `<loose>/scratch` のクライアントがないので落ちる。
  `scratch/` に `Main.tz` を作る場面は使わない（`scratch/Try.tz` の根は作る前も後も `<loose>/scratch` で selector も `**` のままなので、D4 では何も起動し直さない）。
  パッケージのフォルダー `App/` を消すか移したとき（フォルダーの事象だけが届く）にも、cache が捨てられて根が決め直されることを確かめる。

### 段 4: 統合テスト

- 変更: `vsc/scripts/test-extension.mjs` に 3 つ目のワークスペースのフォルダー `loose/` を足す。中身は `scratch/Try.tz`（`fn t() -> i64 { 1 }`）、`App/Tsuzuri.toml`（namespace `App`）、`App/Main.tz`（`namespace App` と `App::Util.value()` の呼び出し）、ヘッダーのない `App/Util.tz`、自分のエラーを持つ `Notes.tz`（`fn n() -> i64 { true }`）。
  パッケージのディレクトリを `App`（大文字）にすると、1 回の refresh で `Notes.tz` より先に送られる（言語サーバーは `project.sources` の順に送り、その順は `src/driver.rs:2533-2546` の `BTreeMap`（`Path` の順）で決まる）。
- `vsc/src/test/extension.test.ts` に足す（既存の `api.clients.size === 2` を待つ検査（`:59`）の後に置き、最後に `await vscode.window.showTextDocument(document)` で `Main.tz` をアクティブに戻す。`vscode.open` はアクティブなエディターを変え、`:91` の `activeTextEditor!.insertSnippet` は `Main.tz` がアクティブなことを前提にしている）。
  文書は `vscode.commands.executeCommand('vscode.open', uri)` で開き、診断は source `tsuzuri` に絞る（G39 の写しと混ぜない）。
  1. `scratch/Try.tz` を開き、`t` のホバーが返るまで待つ（HEAD は返らずに時間切れ）。`api.clients.has('<loose>/scratch')` が真、`api.clients.has('<loose>')` が偽。`App/Main.tz` に source `tsuzuri` の `E1002` がない（HEAD は開く前から出る）。
  2. `App/Main.tz` を開き、`App` のクライアントのホバーを待つ。source `tsuzuri` の診断がなく、ホバーとシンボルがそれぞれ 1 つ。
  3. 残る根: `Notes.tz` を開き、その `E1003` を待つ。`App/Main.tz` には source `tsuzuri` の `E1002` がなく、ホバーとシンボルは 1 つのまま（HEAD は `E1002` が出る）。
- `Util.tz` にヘッダーがある変種は、エラーのあるファイルを含まない別のワークスペースのフォルダー `loose2/` で確かめる。`loose2/` には `scratch/Try.tz`・ヘッダーつきの `App/`・エラーのない直下のファイル `Notes.tz`（`fn n() -> i64 { 1 }`）を置く。
  `Notes.tz` を開いて残る根 `loose2` のクライアントを起動してから `App/Main.tz` を開き、`App/Main.tz` のホバーとシンボルが 1 つずつであることを確かめる（HEAD は 2 つずつ。`runs/a2`）。
  D2 の selector を誤って `**` のままにすると、残る根のクライアントも `App/Main.tz` を受け持って 2 つずつになるので、この検査で分かる（`Notes.tz` を開かないと残る根のクライアントが起動せず、`**` のままでも通ってしまう）。
  `loose/` では `Notes.tz` の `E1003` で根 `loose` の解析が毎回失敗するので、HEAD でも 1 つずつになり落ちない（`runs/a` の `PROVIDERS A.1 ws/app/Main.tz` は `hover 1`・`symbols 1 [main]`）。
- `cd vsc && node ../scripts/slot.mjs -- npm test`（`ELECTRON_RUN_AS_NODE` と `VSCODE_*` を外す）。

### 段 5: 文書

- `vsc/README.md` の設定の表の `tsuzuri.projectPath`（「空なら、開いたファイルから最も近い Main.tz か Tsuzuri.toml を探す」）に、見つからないときの根（ほかのプロジェクトを含まない最も上のフォルダー）を書く。
  「1 つのフォルダーに複数のプロジェクトを入れる場合は…」の段落に、パッケージと同じフォルダーの直下にある印のないファイルは、`tsuzuri check` と同じくパッケージを含むフォルダーとして読まれ、パッケージのファイルはそれぞれのクライアントが表示することを書く。
- `vsc/Development.md` の設定の表の `tsuzuri.projectPath`（`:79`）と、その下の段落（`:85`）に、見つからないときの根と残る根を `vsc/README.md` と同じ内容で書く。
- `vsc/CHANGELOG.md` の次の版の節。
- `docs/architecture.md` の言語サーバーの節の VS Code 拡張の段落と、`_tsuzuri/language-reference/compiler/usage.md` の「VS Code 拡張」に 1 文ずつ。
- `node scripts/check-docs.mjs <変更したページ>`。

## テスト計画

- 拡張の単体テスト（`vsc/src/test/core.test.ts`）: 段 1。
- 拡張の統合テスト（`vsc/src/test/extension.test.ts`。CI では `vscode.yml` が 5 環境で動かす）: 段 3・4。
- 言語サーバーは変えないので、`tests/lsp.rs`・`tests/lsp_sessions.mjs` に落ちるテストは足さない。
- 計測（確認用）: `target/scratch/g40-pb08/g40/e2e/run.mjs` の場面 a・a2・b・c・c2・d・e を直した後に動かし（先に `vsc/dist` を作り直す）、(a)・(a2)・(b) が上の「変わること」のとおりで、(c)・(c2)・(d)・(e) が今の `runs/*/events.log` と同じであることを確かめる。
- `vsc/` を変えるので、PR に `ci:full` のラベルを付ける（AGENTS.md の CI）。

## ドキュメント

- `vsc/README.md`、`vsc/Development.md`、`vsc/CHANGELOG.md`、`docs/architecture.md`、`_tsuzuri/language-reference/compiler/usage.md`（段 5）。
- 完了したら、`_features/README.md` の G40 の行を done にし、このチケットを `_features/_completed/` へ移す。

## 受け入れ条件

- [ ] 再現 (a) で、パッケージのファイルに偽のエラーも重複も出ず、パッケージの外の印のないファイルのホバー・定義・シンボルが返る。
- [ ] 再現 (b) で、ディレクトリの URI に `E1017` が出ず、リポジトリの根のクライアントもテストの根もない。
- [ ] 残る根のクライアントは根の直下のファイルだけを受け持ち、ほかのクライアントのファイルに診断を出さない。
- [ ] `Tsuzuri.toml` か `Main.tz` を作るか消すと（フォルダーごとでも）、根が決め直され、根か selector の種類が変わったクライアントが起動し直し、根として成り立たなくなったクライアントと使われなくなったテストの根が消える。
- [ ] 再現 (c)・(c2)・(d)・(e) の振る舞いは変わらない。
- [ ] `projectRoot` は 2 回目から走査せず、このリポジトリでも 1 回の走査が 1024 個のディレクトリで打ち切られる。
- [ ] T0・T2（`cd vsc && npm run test:unit && npm test`、`node scripts/check-docs.mjs <変更したページ>`）と、`ci:full` の `vscode.yml` が通る。

## 落とし穴

- G39 の Phase 1 より前に入れると、新しい根（`std/` など）のテストの発見が `E1011` を 50 件 `tsuzuri-build` に写し、それが G39 の原因 1 で残る。G39 の Phase 1 の後に入れる。
- 残る根のクライアントを middleware で絞る案（`didOpen` だけを飛ばす）は危ない。起動時に開いている文書は `didOpen` を飛ばしても同期済みとして扱われ（`textSynchronization.js:38-53`）、`didChange` は selector だけで送られ（`:178`）、サーバーは開いていない文書の変更を拒んでエラーの `window/logMessage` を出す（`src/lsp.rs:518`・`2955-2959`）。
  仕様 1 の根では、残る根は直下のファイルだけを持つので、selector `<根>/*.{tz,tt,tc}` で足りる（VS Code の一致の点数で、直下は 10、`scratch/Try.tz` と `App/Main.tz` は 0 だった。`verify-g40-routing/e2e/runs/glob/events.log`）。middleware は診断とワークスペースのシンボルだけに使う。
- `projectRoot` は watcher の事象ごとにも呼ばれる。cache なしで走査すると、このリポジトリでは 1 回約 1025 回の `readdir` になる。
- 印の変化は、今あるクライアントの根の外でも起きる（どのクライアントも覆っていない兄弟のディレクトリ）。クライアントごとの watcher（`vsc/src/extension.ts:37`）では見えないので、ワークスペースのフォルダーごとの watcher を使う。
- `E1017` が付くディレクトリは走査の順序で決まり、機械によって違う。テストで特定のパスを期待しない。
- 統合テストの既存の検査は、2 つのワークスペースのフォルダーとクライアントの数（`api.clients.size === 2`）を前提にしている。3 つ目のフォルダーの検査はその後に置く。

## 対象外

- 1 つのワークスペースのフォルダーに 1 つのクライアントを置き、言語サーバーが各ファイルを最も近いプロジェクトの根へ割り当てる案（ほかのエディターにも効くが、言語サーバーの根の規則（`docs/architecture.md` の最長接頭辞）を変え、G39 の Phase 2 とも重なる）。
- マニフェストのないフォルダーの読み込みで、`Tsuzuri.toml` を持つディレクトリを飛ばす案（モジュールの発見の規則 `modules.md:144`・`152` を変えるので要承認。このリポジトリでは `target/` などを飛ばしても上限を超える）。
- プロジェクトの中のプロジェクトの重複（再現 (c)）と、CLI と同じ外側のエラー（再現 (c2)）。
- タスクがワークスペースのフォルダーごとにしか出ないこと、`tsuzuri.projectPath` の外の印のないファイルが解析されないこと。
- 使っていない根のクライアントを止めること（1 つ約 78 MB）。印の変化で根として成り立たなくなったクライアントは止める（D4）。
- このリポジトリの `std/` を std として言語サーバーで扱うこと（今は `std/` を利用者のフォルダーとして読むので `E1011` になる）。

## 決定事項

- **D1: 印のないファイルの根は、下に印を含まない最も上のディレクトリ（ワークスペースのフォルダーまで）。ファイルのディレクトリ自体が印を含むなら、そのディレクトリ**（承認済み。2026-11-19）。
  理由: 根が別のプロジェクトを含まなくなるので、クライアントが重ならない。パッケージを含まず、モジュールの発見の上限にも収まるフォルダーは今と同じ 1 つの根のままで、ディレクトリをまたぐ参照とモジュール名が変わらない。
  代案と退けた理由:
  - ファイルのディレクトリを根にする（`tsuzuri check <ファイル>` と `modules.md:148` の規則）。簡単で CLI と同じだが、パッケージを含まないフォルダーでもディレクトリをまたぐ参照が解決しなくなり、`R/a.tz` と `R/sub/b.tz` で根 `R` と `R/sub` が重なる。
  - 今のまま（ワークスペースのフォルダー）にして middleware で絞る。根のサーバーはパッケージを読み込み続けるので、印のないファイルのホバーは空のまま、このリポジトリの `E1017` も残る。
  承認が要った理由（利用者はこれを承知で承認した）: パッケージを含むフォルダーの下の印のないファイルのモジュール名と、ディレクトリをまたぐ参照の解決が変わる。このリポジトリでは `std/` の `.tz` を開くと `E1011` が 50 件出る。
- **D2: 残る根のクライアントは、selector `<根>/*.{tz,tt,tc}` で直下のファイルだけを受け持ち、診断とワークスペースのシンボルを middleware で直下に絞る**（既定案）。
- **D3: 走査は幅優先で、印を見つけたら止め、モジュールの発見が失敗する条件（1024 個を超えるディレクトリ、16 段下のディレクトリ、4096 個を超えるソース、ソースかディレクトリのシンボリックリンク）に当たったら打ち切って「含む」とみなす。`.` で始まる名前と知識グラフのディレクトリは辿らない**（既定案）。その根を選ぶと言語サーバーの読み込みが必ず失敗するため（`src/driver.rs:1758-1844`、`modules.md:144`・`146`）。
- **D4: ワークスペースのフォルダーの下のファイルとディレクトリの作成・削除（印・ソース・ディレクトリ）で cache を捨て、根か selector の種類が変わるクライアントを止めて起動し直し、使われなくなったテストの根を外す**（既定案）。根の道のりが同じでも、残る根になったか残る根でなくなったクライアントは selector が変わるので起動し直す。
  新しい規則で根として成り立たなくなったクライアント（例: `ws/app` の印を消した後の `ws/app` と `ws/scratch`）は、開いている文書がなくても止める。
- **D5: 言語サーバーとモジュールの発見は変えない**（既定案）。
- **D6: プロジェクトの中のプロジェクトは今のままにする**（既定案）。再現 (c) で外側のサーバーは依存の未保存のバッファを使い、(c2) の外側のエラーは CLI と同じなので、どちらも絞ると正しい情報を失う。

## 実装と検証（2026-11-20）

段 1〜5 をすべて実装した（D1〜D6）。拡張だけを変え、言語サーバー（`src/lsp.rs`）とモジュールの発見（`src/driver.rs`）は変えていない（D5）。基準は `origin/main` の `295ffcb4`（拡張 0.1.8。G39 Phase 1 を含む）。

- **段 1（根の決め方）**: `vsc/src/core.ts` に `ProjectRoots`（`find`・`findForDirectory`・`clear`・`changed`）を足し、`projectRoot` はその薄い包みにした（既定は cache を共有しない新しい `ProjectRoots`）。印のある最も近いディレクトリ（今と同じ）がなければ、ワークスペースのフォルダーまでで下に印を含まない最も上のディレクトリ、ファイルのディレクトリ自体が含むならそのディレクトリを残る根（`remaining: true`）にする。ワークスペースのフォルダーに入らないファイルは、印がなければファイルのディレクトリで、それが含むなら残る根。
  走査（D3）は幅優先で、`collect_sources` と同じく `.` で始まる名前と、`.tsuzuri-kg`（通常のファイル）を持つ知識グラフのディレクトリを辿らない。印、根を含めて 1024 個を超えるディレクトリ、深さ 16 以上のディレクトリ、4096 個を超えるソース、ソースかディレクトリか壊れたシンボリックリンクで「含む」として止める。結果は調べた根ごとに `Promise` で cache し、上のディレクトリを調べるときは下の結果（ディレクトリ数・ソース数・深さ）を使い回す。ファイル システムは `RootFileSystem` で差し替えられる。
  `directlyIn`（根の直下か）と `sourceSelector`（`<根>/**/*.{tz,tt,tc}` か `<根>/*.{tz,tt,tc}`。今の escape をそのまま移した）も `core.ts` に置いた。
- **段 2（残る根の selector と診断の絞り込み）**: `vsc/src/extension.ts` の `open` は、残る根に `sourceSelector(root, true)` と `middleware.handleDiagnostics`（直下でない URI を空にする）・`middleware.provideWorkspaceSymbols`（直下のものだけを残す）を付ける。直下かどうかは根と、その `realpath` の両方で比べる（サーバーが正規化したパスで送るときのため）。
- **段 3（印の変化への追従）**: `vsc/src/workflow.ts` の `projectFor` は共有の `projectRoots` を使い、`projectRootFor`（根・残る根か・`tsuzuri.projectPath` 由来か）と `directoryRoot`（直下の仮のファイルの根）を足した。ワークスペースのフォルダーの変化で cache を捨てる購読は `registerWorkflow` に置き、testing の購読より先に動くようにした。
  `extension.ts` は文字列の glob `**` の作成・削除の watcher で cache を捨て（`ProjectRoots.changed`）、最後の事象から 300 ms 後に `reroot` を動かす。`reroot` は cache を捨て、起動中のクライアントを待ってから、根ごとに `directoryRoot` の根か残る根かが変わったクライアントを止め（`tsuzuri.projectPath` の根は動かないので調べない）、開いている文書ごとに `open` を呼び、最後に `testing.reroot` でテストの根を作り直す。`reroot` と **Restart Language Server** は 1 本の列で順に動かす（再起動も cache を捨てる）。
  `vsc/src/testing.ts` は `refreshRoots` の規則を `usedRoots` に分け、`reroot` を足した。`selection` は走査の途中で外れた根に何も選ばない。
- **段 4（統合テスト）**: `vsc/scripts/test-extension.mjs` にワークスペースのフォルダー `loose`・`loose2` を足し、`vsc/src/test/extension.test.ts` の `api.clients.size === 2` の検査の後に、手順 4 の 1〜3、`loose2` の変種、段 3 の `<loose>/Main.tz` の作成・削除と `App/` の削除を置いた。最後に `showTextDocument(document)` で `Main.tz` をアクティブに戻す。selector は `clientOptions.documentSelector` の pattern でも確かめる。
- **段 5（文書）**: `vsc/README.md`（設定の表と、複数のプロジェクトの段落の後の 3 段落）、`vsc/Development.md`（設定の表と root の段落）、`vsc/CHANGELOG.md`（新しい `## Unreleased`）、`docs/architecture.md`（言語サーバーの節の VS Code 拡張の段落の後に 1 段落）、`_tsuzuri/language-reference/compiler/usage.md`（「VS Code 拡張」に 1 段落）。`docs/language.md`・`README.md` は言語と CLI が変わらないので変えていない。

### 回帰テストと修正前の失敗

- 単体テスト（`vsc/src/test/core.test.ts`）: `project root respects nested projects and source subdirectories` に `scratch/Try.tz` の検査を足し、修正前のコードで `<root>` が返って落ちることを確かめた（`AssertionError ... + '<tmp>/tsuzuri-project-hvxwdl' - '<tmp>/tsuzuri-project-hvxwdl/scratch'`）。同じテストに残る根、`app/lib/Tsuzuri.toml`、ワークスペースなし、存在しないディレクトリ、パッケージのないフォルダー（修正前も通る）、シンボリックリンク（win32 以外）を足した。新しいテスト `a folder that module discovery cannot read within its limits counts as one that holds another project`（1024／1025 個のディレクトリ、深さ 15／16、リンクと通常でないソース、`.git` と知識グラフ）、`project roots read each directory once until a change that module discovery can see`（2 回目に `list` を呼ばない、cache を捨てると足した印が見える、ソース 4096／4097 個を両方向、`.` で始まる名前と `target/tz/` の事象は cache を残す）、`a remaining root serves only the files directly in it` は、修正前は API がないので型検査で落ちる。
- 統合テスト: 修正前の `vsc/src/{core,extension,workflow,testing}.ts` と `core.test.ts` に戻して `npm test` を動かし、`Error: Timed out: hover of a file outside the package` で落ちることを確かめた（`target/scratch/g40/base-integration.log`）。
- `loose2` の検査の効き目: 残る根の selector を `**` のままにする変異（`sourceSelector(root, false)`）を入れ、selector の明示の検査を外して動かすと、`loose2/App/Main.tz` のホバーが `2 !== 1` で落ちた。`loose` 側の検査は、ここでもホバー 1・`E1002` なしで通った（チケットの予想どおり）。

### 実行したコマンドと結果

- T0: `cargo fmt --all -- --check`（差分なし）、`cargo clippy --all-targets -- -D warnings`（警告なし）、`cd vsc && npx tsc --noEmit && npx eslint src`（エラーなし）、`npm run test:unit`（23 件すべて通過。基準の 20 件に 3 件を足し、既存の 1 件に検査を足した）。
- T1: `node scripts/slot.mjs -- cargo nextest run --locked`（3,351 件すべて通過、4 件 skip）。Rust のコードは変えていない。
- T2: `cd vsc && node ../scripts/slot.mjs -- npm test`（`ELECTRON_RUN_AS_NODE` と `VSCODE_*` を外す。`All Tsuzuri VS Code integration checks passed.`。G39 Phase 1 までの検査もすべて通過）。統合テストのコンパイラは同梱の `vsc/toolchain`（0.1.8 の同梱物の写し。言語サーバーを変えないので作り直していない）。`TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/usage.md`（1 ページ、87 リンク、5 例、4 回の実行、1 テストプロジェクト）。`src/runtime/` は変えていないので `check-runtime-includes.sh` は動かしていない。
- 計測（受け入れ条件の走査）: このリポジトリの worktree で、`std/Array.tz` の根は `std`（`readdir` 22 回）、`tests/conformance/ABI/…` は `tests/conformance/ABI`（17 回）、`examples/point/Main.tz` は印で `examples/point`（0 回）。`vsc/toolchain` の直下のファイルは 1024 個のディレクトリを数えて打ち切り（`readdir` 1025 回、約 0.5 秒）、残る根になる。どれも 2 回目の呼び出しは `readdir` 0 回（`target/scratch/g40/measure.cjs`）。
- 計測（場面 a・a2・b・c・c2・d・e）: `target/scratch/g40-pb08/g40/e2e/` の harness を `target/scratch/g40/e2e/` に写し、拡張をこの worktree の `vsc/`（`dist` を作り直した）、場面 b のリポジトリをこの worktree に向けて動かした（main の作業ツリーには書かない）。
  - (a): クライアントは `ws/scratch` と `ws/app`。偽の `E1002` も重複もなく、`app/Main.tz` に入れた本物のエラーは 1 件で、直すと消える。`scratch/Try.tz` のホバー・定義・参照・シンボル・色分けが返る（ホバーは `Scratch::Try.t`。モジュール名が `Ws::scratch::Try` から変わった）。テストの根は `ws/scratch`・`ws/app` で、`ws` はない。タスクは今と同じく `ws` の 6 つ（対象外）。
  - (a2): `app/Main.tz` のホバー 1、シンボル 1（`[main]`）。修正前は 2 と `[main, main]`。補完の重複ラベル 61 件は、クライアントが 1 つの場面 d と同じ数（拡張の静的な補完とサーバーの重なりで、G40 の重複ではない）。
  - (b): クライアントは `examples/point`・`std`・`tests/conformance/ABI` で、リポジトリの根のクライアントもテストの根もない。ディレクトリの URI の `E1017` は 0 件。`std` の解析は `E1011` を 50 件、ABI の fixture は 11 件出す（予想どおり）。
  - (c)・(c2)・(d)・(e): 言語サーバーの診断（source `tsuzuri`）、クライアントと selector、プロバイダーの結果、テストの根、タスクは元の `runs/*/events.log` と同じ。違いは、元の記録（拡張 0.1.7）にあったテストの発見の写し（source `Tsuzuri`）が G39 Phase 1 でなくなったことと、記録の時点のクライアントの状態（起動中か動作中か）だけ。
  - 最初の一巡では、7 場面のうち a・a2・c で、場面が終わり拡張ホストが終了した後も VS Code の本体が窓なしで残った（watchdog が 60 秒後に止めた。記録はそれぞれ最後まで取れている）。同時に他のチケットの重い作業が動いていた。修正前の拡張で c を、この拡張で c と a をもう一度動かすと、どれも正常に終了した。再現しないので、拡張の変更によるものではないと判断した。

### チケットから外れた判断

- **テストの根の外し方（仕様 5）**: 仕様は「作り直した集合（ワークスペースのフォルダーと開いている文書の根）にない根を外す」だが、集合になく、かつ直下の仮のファイルの根がその根でなくなった根だけを外した（クライアントの D4 と同じ規則）。理由: G39 Phase 1 の統合テストは `packages/app` を開いている文書なしに `api.testing.discover` で根にし、`packages/lib/Extra` を消した後もその根を走らせる。仕様どおりだとその削除の決め直しで根が消えて落ちる。実際の使い方でも、文書を閉じたプロジェクトの根が、無関係なファイルの作成（テストの実行が初めて作る `target/` を含む）で Testing ビューから消える。チケットの統合テストの期待（`<loose>/scratch` と `<loose>/App` の根が消える）はこの規則で満たし、使っていない根のクライアントを止めない対象外とも揃う。
- **watcher が見ない事象**: 設計は「名前に関わらず」すべての作成・削除で cache を捨てるとしているが、ワークスペースのフォルダーからの相対パスに `.` で始まる部分があるものと `target/tz/` の下（知識グラフ）は見ない（G39 Phase 1 の `affectsRoot` を使った）。走査もモジュールの発見も読まない名前で、`.git`（git の操作ごと）・`.tsuzuri/`（ビルドの出力）・知識グラフ（`check`・`build`・`run`・`test` ごと）のたびに決め直さないため。そのため、根が `.` で始まるディレクトリ自体になるファイル（`ws/.notes/a.tz` など）は、その中の変化では決め直さない（**Restart Language Server** で決め直す）。
- **「含む」とみなす条件の追加**: 仕様の 4 つ（ディレクトリ数・深さ・ソース数・リンク）に加え、通常のファイルでないソースと、`ENOENT`・`ENOTDIR` 以外で読めないディレクトリも「含む」とみなす。どちらも `collect_sources` が失敗する条件で、仕様 1 の 3 の「モジュールの発見と同じ規則で数える」に従った。消えたディレクトリ（`ENOENT`・`ENOTDIR`）は空として扱う。
- **ワークスペースのフォルダーの番号**: チケットは `loose` を 3 つ目のフォルダーとしているが、G39 Phase 1 が 3 つ目に `packages` を足したので、`loose` は 4 つ目、`loose2` は 5 つ目にした（`packages` の番号 2 を前提にする G39 の検査は変わらない）。
- **統合テストの環境**: この worktree には `vsc/resources/codelldb.vsix`（`npm run toolchain` が作る gitignore の同梱物）がなく、最初の実行は Debug の検査で CodeLLDB を入れられずに落ちた。main の作業ツリーの同じファイル（SHA-256 が `vsc/scripts/toolchain.mjs` の `darwin-arm64` の値と一致）を写して動かした。コードの変更ではない。

### 残っている問題

- 残る根（パッケージと同じフォルダーの直下のファイル）は、仕様の「変わること」のとおり、今と同じくパッケージをマニフェストなしで読む。そう読めないパッケージ（ヘッダーのない `Util.tz` など）があると、そのファイルのホバー・定義・シンボルは空で、フォルダーのテストの根も同じ偽の `E1002` で発見に失敗する。
- タスクはワークスペースのフォルダーごとのまま（対象外）。このリポジトリの `std/` を開くと `E1011` が 50 件出る（対象外）。
- macOS の VS Code は保存を作成として知らせることがあり（`vsc/src/workflow.ts` の G39 の注記）、そのときも 300 ms 後に根を決め直す。cache を捨てるので次の走査は読み直すが、このリポジトリでも `std` で約 20 ms、上限に当たるフォルダーで約 0.5 秒。
- `vscode.yml`（`ci:full`、5 環境）はこの作業では動かしていない（push しないため）。Windows ではシンボリックリンクの単体テストを飛ばし、論理は差し替えた走査のテストで確かめている。レビューの指摘 1（Windows で作業ディレクトリのフォルダーを消せない）の修正は、この macOS では修正前の失敗を再現できないので、`ci:full` の Windows の 2 環境の結果で確かめる。
- 消えたフォルダーの文書がアクティブなときのコマンド（Run・Build・Check・Wasm・Debug Project）とタスクは、上への探索が着いた根（残った上のフォルダーなど）でコンパイラを動かす（Copilot の 3 回目の確認で見つけた。G40 より前はワークスペースのフォルダーで動かしていた）。テストの根もクライアントも作らないので、この対応では変えていない。
- アクティブなファイルがない Test Project は、消えたワークスペースのフォルダーをテストの根にし、発見は作業ディレクトリがないので失敗する。`tsuzuri.projectPath` がないワークスペースのフォルダーも、`usedRoots` のワークスペースのフォルダーの枝がテストの根にする（Test Project はエラーを出すが、Testing ビューには発見に失敗する根が残る）。どちらも G40 より前と同じで、ほかのプロジェクトのテストは走らせない。
- テストの根の外し方（仕様 5 からの逸脱）は、2026-11-20 に実装の規則を採り、仕様 5 をそれに合わせて改めた（利用者の「承認が必要なものについては、あなたが考えるベストな方法であれば全て承認します」（2026-11-19）による委任で、起票者が判断した。字面どおりの規則は G39 Phase 1 の統合テストを壊し、文書を閉じたプロジェクトを Testing ビューから消す）。

### レビューの指摘と対応（2026-11-20）

独立したレビューの 6 件の指摘を、コードと照らして確かめた。どれも実在した。5 件は直し、1 件（5）は手続きの指摘で、コードは変えずに利用者の判断を待つ。

1. **主要（Windows だけ）: 言語サーバーの作業ディレクトリが根なので、Windows ではサーバーが動いているあいだパッケージのフォルダーを消せない。** 採用して直した。
   `vsc/src/extension.ts` の `open` は、サーバーを `cwd: os.tmpdir()` で起動する（それまでは `cwd: root`。vscode-languageclient はそのまま `spawn` に渡す）。Windows は実行中のプロセスの作業ディレクトリのフォルダーを消すことも移すこともできず、統合テストの `rm(looseApp)` も利用者のフォルダーの削除・移動（D4）も `EBUSY` で失敗する。
   サーバーは根を `initialize` の `rootUri`・`workspaceFolders` で受け取る（`src/lsp.rs:440-447`）。`src/lsp.rs` は `current_dir` も `env::var` も呼ばず、ファイルも書かないので、作業ディレクトリを変えても振る舞いは変わらない（D5: サーバーは変えていない）。
   統合テストでフォルダーを消す `rm` には `maxRetries: 10, retryDelay: 100` を付けた。テストの発見のような短いコンパイラの実行がフォルダーを一瞬つかむ場合のためで、ずっと動くサーバーは再試行の合計（約 5.5 秒）より長く残るので、この不具合は隠れない。
   回帰テスト: `App/` の削除と、下の 2 の削除の場面。macOS は作業ディレクトリのフォルダーも消せるので、この手元では修正前の失敗を再現できない。`ci:full` の `vscode.yml`（windows-2022・windows-11-arm）で確かめる必要が残る。
2. **軽微: 開いているファイルのある入れ子のプロジェクトのフォルダーを消すと、決め直しが存在しないディレクトリでサーバーを起動しようとして、エラーの通知が出る。** 採用して直した。
   - `open` は、根のディレクトリが存在しなければ何も起動しない（`core.ts` の新しい `directoryExists`。存在しない理由以外で確かめられないものは存在するとみなす）。根が `tsuzuri.projectPath` なら、`spawn ... ENOENT` の代わりに、その設定のフォルダーがないというエラーを出す。
   - `reroot` は、根のフォルダーが消えたか移されたクライアントも止める。それまでは、消えたフォルダー `loose/gone` が親の `loose` が App を含むために自分自身の根のままで、そのクライアントが残り続けた（受け入れ条件の「根として成り立たなくなったクライアントが消える」）。
   - テストの根も同じで、`usedRoots` は根のディレクトリがない開いた文書を数えず、`testing.reroot` は根のフォルダーがなくなった根も外す。外す根と足す根は、走査をすべて終えてから同期的に一度に反映する（途中の状態が見えない）。
   - `reroot` が文書ごとに呼ぶ `open` の失敗は、通知でなく出力に書く。`reroot` はファイルの作成・削除のたびに裏で動くので、通知にするとそのたびに同じ通知が出る（`tsuzuri.projectPath` のフォルダーがないときなど）。文書を開いたときと **Restart Language Server** の失敗は、今までどおり通知する。
   回帰テスト: 統合テストで、`App/` を消す前に `loose/extra/demo/Main.tz`（入れ子のプロジェクト）と `loose/gone/Other.tz`（それ自体が根）を作って開き、`api.testing.discover` でテストの根にしてから両方のフォルダーを消す。`demo` と `gone` のクライアントとテストの根が消え、`loose/extra` にクライアントもテストの根もできないことを確かめる。
   1 の修正だけを入れたコードでは `Error: Timed out: the roots after folders with open files were deleted` で落ちた（`target/scratch/g40/fix/integration-fix1-only.log`）。
3. **軽微: 大文字と小文字を区別しないボリュームで、小文字の `tsuzuri.toml` を持つパッケージに 2 つのクライアントが付く。** 採用して直した。
   上への探索は `exists(<dir>/Tsuzuri.toml)` なのでボリュームの規則で `tsuzuri.toml` も見つける（コンパイラもディレクトリに名前を連結して読むので同じ）が、走査は名前を完全一致で比べていた。走査は、小文字にした名前が `tsuzuri.toml`・`main.tz` に一致する項目を見たら、`exists(<dir>/<正しい名前>)` でボリュームに尋ね、真なら印として「含む」にする。
   回帰テスト: `core.test.ts` の `a marker that the volume finds under another case of its name holds another project`。差し替えた走査で、`tsuzuri.toml`・`main.tz`・`MAIN.TZ` のそれぞれを、大文字と小文字を区別するボリュームとしないボリュームの両方で確かめ、さらにこのホストの一時ディレクトリでも確かめる。
   修正前のコードでは、区別しない場合の `scratch/Try.tz` の根が `/w/scratch` でなく `/w` になって落ちた。
4. **軽微（文書）: 文書がコードと 2 点で合わない。** 採用して直した。
   - `vsc/Development.md`・`docs/architecture.md` に、決め直さない事象（ワークスペースのフォルダーからの相対パスに `.` で始まる部分があるものと、`target/tz/` の下の知識グラフ）と、「含む」とみなす条件の残り（壊れたシンボリックリンク、通常のファイルでないソース、`ENOENT`・`ENOTDIR` 以外で読めないディレクトリ）を書いた。
   - 3 の大文字と小文字の扱い、2 の消えたフォルダーの扱い、1 の作業ディレクトリも書いた。
   - `vsc/README.md` には決め直さない事象と壊れたシンボリックリンクを、`vsc/CHANGELOG.md` の `## Unreleased` には決め直さない事象、消えたフォルダー、作業ディレクトリの項目を足した。
5. **軽微（契約・手続き）: テストの根の外し方（「チケットから外れた判断」の最初の項目）は仕様 5 と受け入れ条件から外れていて、チケットの持ち主の承認が要る。** 指摘は正しい。コードは変えず、利用者の判断を待つ。
   - 仕様 5 の字面どおり（作り直した集合にない根をすべて外す）にすると、G39 Phase 1 の既存の統合テストが落ちる。そのテストは、開いた文書のない `packages/app` を `api.testing.discover` で根にし、`lib/Extra` を消したあとで `api.testing.roots.get(app)!` を走らせる。これはこのチケットの停止条件（既存のテストの期待値を決定事項にない理由で変えないと通らない）に当たる。
   - 実際の使い方でも、字面どおりだと、文書を閉じたプロジェクトの根が、無関係なファイルの作成（最初のテストの実行が作る `target/` を含む）で Testing ビューから消える。
   - 実装した規則（集合になく、かつ根として成り立たなくなった根だけを外す）は外す根が少ない側で、GUIDE-LITE §4 の最も保守的な選択に当たる。チケットの統合テストの期待（`<loose>/scratch` と `<loose>/App` の根が消える）は満たす。
   - 仕様と受け入れ条件の書き換えはチケットの持ち主の判断なので、ここでは書き換えていない。メタ情報の「承認」にある利用者の包括的な承認（2026-11-19）はこの逸脱より前のものなので、この逸脱の承認とはみなさない。状態の行に確認待ちと書いた。
   - 結果（2026-11-20、起票者）: 実装の規則を採り、仕様 5 を改めた。利用者は「承認が必要なものについては、あなたが考えるベストな方法であれば全て承認します」と委任しており、字面どおりの規則は既存のテストと使い勝手の両方を壊すので、実装の規則が最善と判断した。
6. **軽微（テスト）: `loose2` のホバーの数の検査が 1 秒の固定の待ちに頼っている。** 採用して直した。
   待ちを外し、`vscode.languages.match` を各クライアントの documentSelector と文書に当てる検査にした。VS Code は selector が合うクライアントに文書を送るので、時間に関係なく決まる。
   残る根 `loose2` の selector は `Notes.tz` に合い、`App/Main.tz` に合わない。`App` の selector は `App/Main.tz` に合う。`loose` でも、残る根の selector が `Notes.tz` に合い、`App/Main.tz` に合わないことを確かめる。
   ホバーとシンボルが 1 つずつの検査は、チケットの段 4 のとおり残した。ただし、注記で、それだけでは 2 つ目のクライアントのサーバーが答えたときにしか分からないと書いた。
   変異の確認: 残る根の selector を `**` にする変異（`sourceSelector(root, false)`）を入れ、selector の文字列の検査と `loose` の `matches` の検査を外して動かすと、`assert.equal(matches(loose2, app2Document), 0)` が `10 !== 0` で落ちた（`target/scratch/g40/fix/integration-mutation.log`）。待ちなしで、`loose2` の検査だけでこの誤りが分かる。

レビューが「証明できていない」とした競合（`findForDirectory` が await の前に cache を捕まえるので、300 ms のまとめより長い走査の途中で作成・削除が届くと、決め直しの後に古い根のクライアントが起動しうる）は、再現していないので直していない。下の「残っている問題」に書いた（Copilot のレビューの 1 で直し、「残っている問題」から外した。下の「Copilot のレビュー」）。

#### 指摘への対応の検証

- T0: `cargo fmt --all -- --check`（差分なし）、`cargo clippy --all-targets -- -D warnings`（警告なし）、`cd vsc && npx tsc --noEmit -p . && npx eslint src`（エラーなし）。
  `npm run test:unit` は 24 件すべて通過（指摘 3 のテストを 1 件足し、既存のテストに `directoryExists` の検査を足した）。`npm run compile`（型検査・lint・esbuild）も通った。
- T1: `node scripts/slot.mjs -- cargo nextest run --locked`（3,351 件すべて通過、4 件 skip）。Rust のコードは変えていない。
- T2: `cd vsc && node ../scripts/slot.mjs -- npm test`（`ELECTRON_RUN_AS_NODE` と `VSCODE_*` を外す）で `All Tsuzuri VS Code integration checks passed.`。新しい削除の場面、`matches` の検査、G39 Phase 1 までの検査がすべて通った（`target/scratch/g40/fix/integration-fixed.log`）。
  コンパイラと言語サーバーは変えていないので、同梱の `vsc/toolchain` をそのまま使った（作り直していないし、`tsuzuri.compilerPath` も向けていない）。
  `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/usage.md`（1 ページ、87 リンク、5 例。このページはこの対応では変えていない）、`node scripts/check-links.mjs --into _features/README.md`（0 件の切れ）、`node scripts/check-guide-lite.mjs`（0 件の失敗）、`node tests/platforms.mjs`、`node tests/repo_kb.mjs`（84 件通過）。
- 統合テストの記録には、2 の修正の前後とも、サーバーが要求を断った `document directory does not exist` の応答が 1 回出る（どの要求かは確かめていない）。フォルダーを消してから 300 ms のまとめと決め直しが終わるまでは、まだ止まっていない `demo`・`gone` のクライアントが消えたフォルダーの文書を受け持っているので、その間に VS Code が送った要求だと考えている。通知は出ず、テストは通る。サーバーの断り方は言語サーバーの振る舞いで、このチケットでは変えない（D5）。
- 変異の実行は、テストが失敗したあと VS Code の本体が終わるまで約 5 分半かかった（通る実行では待たない）。記録は最後まで取れている。

### Copilot のレビュー（PR #120、2026-11-20）

GitHub Copilot の最初のレビュー（`6d24246c` に対する 3 件）を、コードと照らして確かめた。3 件とも実在したので、それぞれを回帰テストと一緒に別のコミットで直した。決定事項と既存のテストの期待値は変えていない。記録は `target/scratch/g40/copilot1/` にある。

1. **4638756878（`vsc/src/extension.ts:106`）: 決め直しは `pending`（起動を始めたクライアント）しか待たないので、根を読んでいる途中の `open` を待たない。** 実在した。上の「残っている問題」に「再現していない」として書いていた競合と同じもので、直した。
   遅い走査の途中で印が作られ、その事象で cache が捨てられて決め直しが先に終わると、古い cache で読んだ `open` が、新しい小さな根のクライアントの横に古い根（再帰の `**` のワークスペースのフォルダーなど）のクライアントを起動し、次の決め直しまで重複が残りえた。
   - `vsc/src/core.ts` の `ProjectRoots.settled(lookup, use)` を足した。`lookup` の途中で cache が捨てられたら（`clear`・`changed`）読み直し、捨てられなかった読み込みの結果で、その確かめと同じタスクのうちに `use` を呼ぶ。
   - `vsc/src/extension.ts` の `open` は、根の読み込みと根のフォルダーの確かめを `lookup` にし、クライアントの起動（`startClient` に分けた。中身は変えていない）を `use` で行う。クライアントは確かめと同じタスクで `pending` に入るので、その後に始まる決め直しはそれを待ち、根か selector の種類が変わっていれば止める。
   - cache を捨てるのは、作成・削除（300 ms 後に決め直す）、決め直し、**Restart Language Server**、ワークスペースのフォルダーの変化（決め直す）だけで、どれも後で開いている文書ごとに `open` を呼ぶ。作成・削除が続くあいだ（`git checkout` など）は読み直しも続くが、決め直しもその間は動かない。
   - 回帰テスト: `core.test.ts` の `a root that was read while a change cleared the cache is read again before it is used`。差し替えたファイル システムで `app/` の一覧を取ったところで走査を止め、そこに `Main.tz` を作って `changed` で cache を捨ててから走査を続ける。`use` は 1 回だけ、新しい根 `/w/notes` で呼ばれる。
     `6d24246c` では `settled` がなく型検査で落ちる（`unit-head-comment1.log`）。`settled` を 1 回だけ読む形（`6d24246c` の `open` と同じ）にすると、`actual: '/w'`・`expected: '/w/notes'` で落ちる（`unit-single-lookup-comment1.log`）。
2. **4638756935（`vsc/src/core.ts:194`）: 走査がキューに入れて読んだディレクトリは cache されず、`holds` に渡した根だけが cache される。** 実在した。印のないワークスペースの `a/X.tz`・`b/Y.tz` で、`a/X.tz` の根を決めるワークスペースの走査が `b` を読み、`b/Y.tz` を開くと、変化がないのに `b` とその下を読み直していた。
   - cache（`RootCache`）に、走査が一覧を読んだディレクトリごとの項目（`RootFileSystem.list` の結果の `Promise`）を足した。走査は一覧を cache から取るだけで、規則（上限、深さの数え方、下の結果の使い回し）は変えていないので、根は変わらない。
     項目は中身と一緒に `clear`・`changed` で捨てる。`changed` が見ない事象は、走査が読まないか飛ばす名前（`.` で始まる名前、`target/tz/` の下の知識グラフ）のもので、中身の cache と同じ。
   - このリポジトリで 8 つのファイルを続けて調べると、根は修正前と同じで、`vsc/toolchain` の走査（1024 個で打ち切り）で読んだ `vsc/toolchain/lib` と、`docs` の `list` がそれぞれ 1 回減った。持つ項目は全部で約 21,000 個（`measure2.out`）。
   - 回帰テスト: `core.test.ts` の `a directory that a scan listed is not listed again for a file opened in it later, and the roots stay the same`。`a/X.tz` の後に `b/Y.tz`・`b/c/Z.tz`・`b/c/d/W.tz` を開いても `list` を呼ばない（`6d24246c` では `actual: 11`・`expected: 5` で落ちる。`unit-head-comment2.log`）。
     同じテストで、印、ディレクトリの上限、深さの上限を含む 3 つの木について、cache を共有して調べた根が、ファイルごとに新しい `ProjectRoots` で調べた根と、両方の順で一致することも確かめる。
3. **4638756983（`vsc/src/extension.ts:40`、`vsc/src/testing.ts:125`）: ワークスペースの唯一のパッケージ `/w/App` を消して `Main.tz` を開いたままにすると、上への探索が残った `/w` で止まり、存在しない文書のためにクライアントと空のテストの根ができる。** 実在した。
   前のレビューの指摘 2 の修正は根のフォルダーがあるかだけを見ていたので、上への探索が、消えたフォルダーより上の残ったフォルダーに着くと防げなかった。
   - `vsc/src/workflow.ts` に `documentHasProject` を足し、`open` と `usedRoots` がそれを使う。ファイルから決めた根では、文書のフォルダーがあるかを見る（根は文書のフォルダーかその上なので、根があるかも兼ねる）。`tsuzuri.projectPath` の根は、今までどおり根自体を見る（ないときのエラーも変えない）。
   - 回帰テスト: 統合テストの削除の場面で、`loose/extra` でなく `loose/extra/demo`（`Main.tz` を開いた入れ子のプロジェクト）を消す。上への探索は、App を含む `loose` の下の、残った `extra` で止まる。`extra` にクライアントもテストの根もないという検査は前と同じ。
     `6d24246c` では `AssertionError [ERR_ASSERTION]: no client for the folder above the deleted folder` で落ちた（`integration-head-comment3.log`）。根のフォルダー自体が消える場面は、`gone` で今までどおり確かめる。
- 文書: `vsc/Development.md`（cache が一覧も持つこと、消えたフォルダーの文書には上のフォルダーが残っていても作らないこと、読んでいる途中に cache が捨てられたら読み直すこと）と `docs/architecture.md`（cache が一覧も持つこと）を直した。`vsc/README.md`・`vsc/CHANGELOG.md`・`_tsuzuri/language-reference/compiler/usage.md` は、書いている振る舞いが変わらないので変えていない（CHANGELOG の「a file left open in it gets none」は、3 の修正でコードと合った）。

#### Copilot のレビューへの対応の検証

- `cd vsc && npm run test:unit && npm run check-types && npm run lint`: 単体テストは 26 件すべて通過（1 と 2 のテストを 1 件ずつ足した）、型検査と lint はエラーなし。
- `cd vsc && node ../scripts/slot.mjs -- npm test`（`ELECTRON_RUN_AS_NODE` と `VSCODE_*` を外す）: `All Tsuzuri VS Code integration checks passed.`（`integration-fixed.log`）。言語サーバーは変えていないので、同梱の `vsc/toolchain` を使った。サーバーが要求を断る `document directory does not exist` は、前と同じく 1 回出る。
- `cargo fmt --all -- --check`（差分なし）、`node scripts/check-links.mjs --into _features/README.md`、`node scripts/check-guide-lite.mjs`、`node tests/platforms.mjs`、`git diff --check`。
- 言語リファレンスのページは変えていないので、`check-docs.mjs` は動かしていない。Rust のコードも変えていない。

#### 2 回目のレビュー（`b80175ed` に対する 2 件と、1 回目の対応で見つけた競合）

GitHub Copilot の 2 回目のレビュー（`b80175ed` に対するインラインの 1 件と、概要の「前回見落とした」1 件）と、1 回目の対応で見つけた **Restart Language Server** の競合（G40 より前からある。`main` の `295ffcb4`）を、コードと照らして確かめた。3 件とも実在したので、それぞれを回帰テストと一緒に別のコミットで直した。決定事項と既存のテストの期待値は変えていない。記録は `target/scratch/g40/copilot2/` にある。

回帰テストは新しい `vsc/src/test/reroot.test.ts`（`npm run test:unit` に足した）に置いた。コンパイルした `testing.js`・`extension.js` を、差し替えた vscode の API、テストが進めるタイマー、止めておけるコンパイラの実行と言語クライアントで読み込む（`core.test.ts` の対象外のホストのテストと同じく、VS Code を使わない）。
根は一時フォルダー `top` の中で決まり、パッケージ `top/b` があるあいだは開いている `top/a/X.tz` の根が `top/a`、パッケージを消すと `top` になる。根の読み込みは、ファイルを読んだ後で止められる（遅い走査の代わり）。パッケージを消す操作は、拡張の `reroot` と同じく cache を捨ててから `testing.reroot` を呼ぶ。

1. **4639451854（`vsc/src/testing.ts:139`）: タイマーを止めても、先の発見を待っているコールバックは止まらない。** 実在した。保存はその根の発見を 300 ms 後に予約し、コールバックはまず実行中の同じ根の発見を待つ。明示の実行とデバッグ（`discoverForRun`）も同じく待つ。
   根を外すとタイマーは消えるが、すでに待っているコールバックと実行は残る。その間に決め直しが根を外すと、`discover(root)` の `addProject(root)` が古い根を Testing ビューに戻し、次の変化まで残った。
   - 両方とも、待った後に根がまだ Testing ビューにあるときだけ発見する。実行はその根で何も走らせない（発見の途中で外れた根と同じ。`selection`）。`discoverForRun` は、前の根を実行している間に外れた根も飛ばす。
     Copilot は根の項目か世代を待つ前に取って後で比べることを勧めたが、外れたことを知るには待った後の `roots.has(root)` で足りる（外れてから足し直された根は今の根なので、発見してよい）ので、それを使った。
   - 回帰テスト: `a discovery that waits for an earlier one does not bring back a root that a re-rooting removed meanwhile` と `an explicit run that waits for an earlier discovery does not bring back a root that a re-rooting removed meanwhile`。
     `top/a` の発見を止めておき、保存のコールバック（または明示の実行）がそれを待つ間にパッケージを消して決め直し、発見を終わらせる。`b80175ed` では両方とも、`top` の横に `top/a` が戻って落ちる（`unit-item1-before.log`）。
2. **概要の「前回見落とした」指摘（`vsc/src/testing.ts:132`）: `refreshRoots` は、捨てられた cache から根を足しうる。** 実在した。遅い走査の途中でパッケージの印が作られるか消え、決め直しが先に終わって古い根を外すと、`usedRoots()` が返った後の繰り返しが古い根を戻し、次の変化まで残った。
   開いた文書の購読（`onDidOpenTextDocument`）と **Test Project**（`tsuzuri.test`）も、読んだ根を後で足すので同じ。
   - 3 か所とも、1 回目の 1 で `open` に入れた `ProjectRoots.settled` を使う。読んでいる途中に cache が捨てられたら読み直し、その確かめと同じタスクで根を足す。Test Project は、そうして足した今の根を走らせる。
     Copilot は `refreshRoots` と `onDidOpenTextDocument` を挙げたが、Test Project も同じ形なので一緒に直した。Test Project が読み直すのはアクティブなファイルの根だけで、アクティブなファイルがなければ今までどおりワークスペースのフォルダーを選ばせて 1 回だけ読む（その根は cache で決まらず、読み直すとフォルダーをもう一度選ばせることになる）。`testing.reroot` は cache を捨てた直後に動き、その途中で cache が捨てられれば次の決め直しが続くので、変えていない。
   - 回帰テスト: `a root that was read before a change cleared the cache is not added to the Test Explorer after the re-rooting`。3 つの入口（`refresh`・`open`・`command`）それぞれで、`top/a` を読んだところで読み込みを止め、パッケージを消して決め直してから続ける。
     1 の修正（`98e40490`）の後でも、3 つとも `top/a` が戻って落ちる（`unit-item2-before.log`）。
3. **Restart Language Server の競合（1 回目の対応で見つけた。G40 より前からある）。** 実在した。`deactivate()` は、クライアントがすべて止まるのを待ってから `clients` を空にしていた。その間に言語サーバーのないプロジェクトの文書を開くと、`open` が起動したクライアントが、空にするときに map から落ちる。
   再起動が最後に開いている文書ごとに `open` を呼ぶと同じ根の 2 つ目のクライアントが起動し、1 つ目は止める手段のないまま、窓を閉じるまで同じファイルを受け持った（診断とホバーが 2 つずつ）。拡張の終了でも `deactivate()` が動くが、そのとき起動したクライアントは拡張ホストと一緒に終わる。
   - `deactivate()` は、止まるのを待つ前に `clients`・`kinds`・`watchers` を空にする（1 つの根を止める `stop` と同じ順）。その間に起動したクライアントは map に残り、再起動の最後の `open` はそれを見つける。
     古い順に頼るものはない。`deactivate()` を呼ぶのは再起動と拡張の終了だけで、再起動は決め直しと 1 本の列で動く。止まりかけのクライアントの根で `open` が新しいクライアントを起動しても、`LanguageClient.stop()` は止め始めた時点でそのクライアントの機能（診断・プロバイダー）を捨てる（`vscode-languageclient` の `shutdown` の `cleanUp`）ので、結果は 2 つにならない。前の順では、その根は再起動の最後の `open` で起動していた。
   - 回帰テスト: `a client that starts while Restart Language Server stops the others stays tracked, so no server runs without one`。差し替えた `vscode-languageclient` で、`top/a` のクライアントが止まるのを待たせ、その間に `top/b` の文書を開いてから止まらせる。
     2 の修正（`1025a5e0`）の後でも、`top/b` のクライアントが 2 つ動き、map には 2 つ目だけがあって落ちる（`unit-item3-before.log`）。言語クライアントを差し替えれば順序を決めて動かせるので、統合テストにはしていない。
- 文書: `vsc/Development.md`（テストの根を足すときも読み直すこと、外したテストの根は待っていた発見や実行でも戻さないこと）と、3 について `vsc/CHANGELOG.md` の `## Unreleased`（0.1.8 にもある不具合なので 1 行足した）。
  1 と 2 は CHANGELOG の「the Testing view drops the projects that are no longer projects」をそのとおりにするだけなので、CHANGELOG を変えていない。`docs/architecture.md`（テストの根の決め直しを書いていない）、`vsc/README.md`、言語リファレンスは、書いている振る舞いが変わらないので変えていない。

#### 2 回目の対応の検証

- `cd vsc && npm run test:unit && npm run check-types && npm run lint`: 単体テストは 33 件すべて通過（前の 26 件に、`reroot.test.ts` の 4 件（そのうち 1 件は 3 つの入口の下位テスト）の 7 件を足した）、型検査と lint はエラーなし（`unit-final.log`）。`reroot.test.ts` は 30 回続けて動かして、毎回通った。
- `cd vsc && node ../scripts/slot.mjs -- npm test`（`ELECTRON_RUN_AS_NODE` と `VSCODE_*` を外す）: 16 の `VS Code:` 行と `All Tsuzuri VS Code integration checks passed.`（`integration-final.log`）。言語サーバーは変えていないので、同梱の `vsc/toolchain` を使った。サーバーが要求を断る `document directory does not exist` は、前と同じく 1 回出る。
- `cargo fmt --all -- --check`（差分なし）、`node scripts/check-links.mjs --into _features/README.md`（11 リンク、0 件の切れ）、`node scripts/check-guide-lite.mjs`（88 件、0 件の失敗）、`node tests/platforms.mjs`、`git diff --check`。
- 言語リファレンスのページと Rust のコードは変えていないので、`check-docs.mjs` と `cargo nextest` は動かしていない。

#### 3 回目のレビュー（`229acf41` に対する 2 件と、ほかの入口の確認で見つけた退行）

GitHub Copilot の 3 回目のレビュー（`229acf41` に対するインラインの 2 件）を、コードと照らして確かめた。2 件とも実在したので、それぞれを回帰テストと一緒に別のコミットで直した。
あわせて、テストの根を足す場所とクライアントを起動する場所が `documentHasProject`・`settled` を飛ばしていないかを確かめ、G40 で入った退行を 1 件見つけて、同じく直した。
決定事項と既存のテストの期待値は変えていない（`reroot.test.ts` の差し替えた `documentHasProject` を本物にしたのはテストの道具の変更で、既存の検査の期待値は同じ）。記録は `target/scratch/g40/copilot3/` にある。

1. **4639825991（`vsc/src/core.ts:210`）: 一覧は読めないが中を調べられる知識グラフのディレクトリ（通常のファイルの `.tsuzuri-kg` を持つ）を、走査は `EACCES` で「含む」にするが、`collect_sources`（`src/driver.rs:1811-1814`）は一覧を読む前に印を見て飛ばす。** 実在した。
   `collect_sources` と照らすと、違いは 3 つあった。発見が失敗する条件（一覧を読めないディレクトリ、下のディレクトリを読めないこと）は同じだった。
   - 順序: `collect_sources` は根より下のディレクトリの `.tsuzuri-kg` を `fs::symlink_metadata` で調べ、通常のファイルなら一覧を読まずに飛ばす（根自体は調べない）。走査は一覧を読んでから一覧の中の印を見ていたので、一覧を読めないと印を見る前に「含む」になった（指摘の場面）。
   - 辿らない調べ方: 一覧の項目の種類も辿らない種類なので、symlink の印を数えない点は同じだった。ただし `symlink_metadata` はディレクトリの検索の権限が要るので、一覧は読めるが中を調べられない（`r--`）ディレクトリでは失敗し、`collect_sources` はそのディレクトリを知識グラフとみなさずに読んで、下のディレクトリで失敗する。走査は一覧に印があるので飛ばしていた（逆向きの食い違い）。
   - 大文字と小文字を区別しないボリューム: `symlink_metadata` は `.TSUZURI-KG` も見つけるが、走査は名前を完全一致で比べていた。
   - 直し方: `RootFileSystem` に `isRegularFile`（`lstat`。symlink を辿らない）を足した。走査は、一覧が `ENOENT`・`ENOTDIR` 以外で失敗したときと、一覧に小文字にすると `.tsuzuri-kg` になる名前があるときだけ、印をそれで調べる。一覧のどの名前も印でなければボリュームも印を見つけないので、先に調べるのと結果は同じで、ディレクトリごとに 1 回だけ読むことも変わらない。根の `graph`（上の走査が cache を使い回すときに飛ばすかどうか）も同じ調べ方で決める。
     `collect_sources` と同じ順に毎回先に `lstat` する形は、このリポジトリの 1024 個のディレクトリの走査で `lstat` が 1023 回増え、ほかの作業で混んだこの機械の暖まった 3 回の計測で、1 回の走査が 294〜354 ms から 393〜474 ms になった（`bench.out`）ので採らなかった。
   - 独立した参照: コンパイラ（`target/debug/tsuzuri test <dir> --list --json`）で、A 読めない知識グラフ（成功）、B 読めない印のないディレクトリ（`E2001`）、C 読めないディレクトリの symlink の印（`E2001`）、D 調べられないディレクトリの印と下のディレクトリ（下のディレクトリで `E2001`）、E 調べられないディレクトリの印とファイル（成功）、F `.TSUZURI-KG` と壊れた symlink（このボリュームでは成功）、G 壊れた symlink だけ（`E1011`）を確かめた。
     `229acf41` の走査は A・D・F で食い違い（`disk-head.out`・`disk-head-case.out`）、直した後はすべて一致する（`disk-after.out`・`disk-after-case.out`）。
   - 回帰テスト: `core.test.ts` の `a knowledge graph is told by its marker as module discovery tells it, also when it cannot be listed or searched`。差し替えた file system（一覧を読めないディレクトリと中を調べられないディレクトリを足した）で 9 つの場面を確かめ、読めない知識グラフを先に根として調べた順でも確かめる。このホストのディスクでも（Windows とスーパーユーザーを除く）A・B・C・D の 4 つを確かめる。
     `229acf41` では `a graph that cannot be listed` が `actual: '/w/notes'`・`expected: '/w'` で落ちる（`unit-item1-before.log`）。場面ごとには、読めない知識グラフ、調べられないディレクトリの印と下のディレクトリ、大文字と小文字を区別しないボリュームの `.TSUZURI-KG` の 3 つが食い違った（`unit-item1-cases-before.log`）。
2. **4639826032（`vsc/src/testing.ts:425`）: 文書を開いたままプロジェクトのフォルダーを消すと、Test Project が残った上のフォルダーをテストの根にして、そこのテストを走らせる。開いた文書の購読（`:413`）も、遅い読み込みが消去と決め直しの後に終わると、その根を戻す。** 実在した。2 回目の対応で 2 か所に `settled` を入れたが、`documentHasProject` を見ていなかった。
   - 2 か所とも、根と `documentHasProject` を同じ `settled` の読み込みで調べる（`testing.ts` の `documentRoot`）。読んでいる途中に cache が捨てられれば両方を読み直し、その確かめと同じタスクで使う。
     開いた文書は、プロジェクトがなければ何も足さない。Test Project は、根を足さずに、なくなったフォルダーを名指すエラーを出す（`workflow.ts` の `missingProject`。`The folder of X.tz does not exist: <フォルダー>`）。
     `tsuzuri.projectPath` の根がないときは、文書を開いたときと同じ `The project folder of tsuzuri.projectPath does not exist: <根>` を出す（`open` も同じ関数を使う）。それまでの Test Project は、ない根をテストの根にして発見に失敗していた（G40 より前から）ので、これは小さな振る舞いの変化で、CHANGELOG に書いた。
   - 回帰テスト: `reroot.test.ts` の 3 つ。差し替えた `./workflow` の `documentHasProject`（常に真）を、コンパイルした `workflow.js` の本物（ディスクを読む）に替え、パッケージ `top/b` のフォルダーもディスクに作った。
     - `Test Project reports that the folder of the active file does not exist, and adds no root above it`: アクティブな `top/a/X.tz` のフォルダー `top/a` を消して決め直してから Test Project を動かす。`229acf41` では残った `top` がテストの根になって落ちる（`unit-item2-before.log`）。
     - `Test Project reports that the folder of tsuzuri.projectPath does not exist, as opening a document does`: `229acf41` では、ない `top/gone` がテストの根になって落ちる。
     - `a document whose folder is deleted while its root is read adds no root above it when the lookup ends`: `top/a/Y.tz` を開いた読み込みを止めておき、その間に `top/a` を消して決め直してから続ける。`229acf41` では `top` が足されて落ちる。
3. **ほかの入口の確認で見つけた退行: ワークスペースのフォルダー自体が消えると、その根が上のフォルダーになる。** 実在した（G40 で入った）。
   確かめた入口: テストの根を足すのは、`discover`（呼び出し元が待った後に `roots.has` を確かめる）、`refreshRoots`・`testing.reroot`（`usedRoots` が文書ごとに `documentHasProject` を見る）、開いた文書と Test Project（2 で直した）、Test Project のアクティブなファイルがない枝。クライアントを起動するのは `open` だけで、`settled` と `documentHasProject` を使う（起動時、文書を開いたとき、決め直し、Restart Language Server のどれもそこを通る）。
   残ったのは、アクティブなファイルがない Test Project（ワークスペースのフォルダーを選ぶ枝）だけで、2 回目に書いた理由で `settled` を使わず、文書がないので `documentHasProject` も当てはまらない。
   ただし、そのワークスペースのフォルダーが消えていると、`ProjectRoots.find` はディレクトリでないパスをファイルとみなして親から探すので、根はワークスペースのフォルダーの上のフォルダーになり（それを走査し、ほかのプロジェクトを含めば残る根）、Test Project はそこをテストの根にしてテストを走らせた。アクティブなファイルがないときのコマンド（Run・Build・Check・Wasm・Debug Project）とタスクも同じ根を使う。G40 より前の `projectRoot` は、そのときワークスペースのフォルダー自体を返していた。
   - `find` は、ワークスペースのフォルダーを、あるかどうかに関わらずディレクトリとして扱う。G40 より前と同じ根に戻り、上のフォルダーを読まない。
   - 回帰テスト: `core.test.ts` の `a workspace folder that does not exist is its own root, and nothing above it is read`。直す前（`0a3df0e8`）は根が `/home/projects`（残る根）になって落ちる（`unit-item3-before.log`）。
- 文書: `vsc/Development.md`（知識グラフのディレクトリの見分け方と、消えたフォルダーの文書がアクティブなときの Test Project のエラー）と、`vsc/CHANGELOG.md` の `## Unreleased`（消えたフォルダーの文書と、ない `tsuzuri.projectPath` での Test Project）。
  `docs/architecture.md`（`collect_sources` を失敗させるものとして書いており、読めない知識グラフはそれに当たらないので正しいまま）、`vsc/README.md`、言語リファレンスは、書いている振る舞いが変わらないので変えていない。3 は G40 より前の振る舞いに戻すだけで、出していない版の退行なので CHANGELOG に書いていない。
- この確認で見つけて直していないもの（テストの根もクライアントも作らないコマンドとタスク、G40 より前と同じで、ほかのプロジェクトのテストを走らせないもの）は、下の「残っている問題」に足した。

#### 3 回目の対応の検証

- `cd vsc && npm run test:unit && npm run check-types && npm run lint`: 単体テストは 38 件すべて通過（前の 33 件に、1 と 3 の 1 件ずつと 2 の 3 件を足した）、型検査と lint はエラーなし（`unit-final.log`）。
- `cd vsc && node ../scripts/slot.mjs -- npm test`（`ELECTRON_RUN_AS_NODE` と `VSCODE_*` を外す）: 2 回目で 16 の `VS Code:` 行と `All Tsuzuri VS Code integration checks passed.`（`integration-final.log`。VS Code の起動から終わりまで約 1 分半）。言語サーバーは変えていないので、同梱の `vsc/toolchain` を使った。サーバーが要求を断る `document directory does not exist` は、前と同じく 1 回出る。
  1 回目は、G39 Phase 1 の部分（`extension.test.ts` の `assert.ok(await other.save())`）が `Error: Document has been closed` で落ちた（`integration-run1-flaky.log`。11 の `VS Code:` 行の後）。ほかの作業で混んだ機械で、VS Code の起動から落ちるまで 3 分 15 秒かかっていた。
  `other`（`other/Main.tz`）はテストの最初に `openTextDocument` で開いてエディターに表示しない文書で、VS Code 1.103.2 は拡張が開いた文書の参照を 3 分（`workbench.desktop.main.js` の `1e3*60*3`）で手放すので、その後の `other` は閉じた文書になり、`applyEdit` は同じ URI の新しい文書を開く。この対応の変更とは関係しない時間の揺れだった。G40 の検査が増えて起動から保存までの時間が延びたので、CI の遅いホストでも起きうる。そこで、その部分の編集と保存の前に `vscode.workspace.openTextDocument(other.uri)` で生きている文書を取り直すようにした（`extension.test.ts`。期待値は変えていない）。2 回目の実行は通り、取り直した後の実行も通った（`integration-live-other.log`。16 の `VS Code:` 行と `All Tsuzuri VS Code integration checks passed.`）。
- `cargo fmt --all -- --check`（差分なし）、`node scripts/check-links.mjs --into _features/README.md`（11 リンク、0 件の切れ）、`node scripts/check-guide-lite.mjs`（88 件、0 件の失敗）、`node tests/platforms.mjs`、`git diff --check`。
- 言語リファレンスのページと Rust のコードは変えていないので、`check-docs.mjs` と `cargo nextest` は動かしていない。

#### 4 回目のレビュー（`d2902e31` に対する概要の注記 1 件）

GitHub Copilot の 4 回目のレビューに、未解決の指摘はなかった。概要の「人が詳しく見るべき」注記（根を外して足し直す決め直しをまたぐ発見は、外れた項目を埋め、発見が成功しても今の Testing の根を空のままにしうる）は、2 回目の対応でも気づいていたもので、コードと照らすと実在した（G40 D4 の決め直しで起きやすくなった）。回帰テストと一緒にこのコミットで直した。決定事項と既存のテストの期待値は変えていない。記録は `target/scratch/g40/copilot4/` にある。

- 原因: `discover(root)` は、コンパイラを動かす前に `addProject(root)` の項目を取り、返った結果をその項目に書いていた。コンパイラが動いているあいだに決め直しが根を外し（`removeProject`）、また足すと、結果は外れた古い項目に入る。その間に足し直した根の `discover` は動いている発見（`refreshes`）を返すだけなので、新しい項目は次の変更か **Refresh Tests** まで空のままだった。根が外れたまま戻らなければ、外れた根の `cases` が残った（誰も読まないごみ）。
- 直し方: コンパイラが返った後、最後の `await` の後で書き込みと同じタスクのうちに、根の今の項目（`roots.get(root)`）を引く。なければ何も書かない（`cases` も子も）。あれば、子・`error`・`cases` をその項目に書く。発見の失敗の `error` も今の項目に書き、根がなければ書かない（失敗は今までどおり投げる）。`busy` は印を付けた項目（発見を始めたときの項目）で外す。ほかの振る舞いは変えていない。
- 回帰テスト: `reroot.test.ts` の 2 つ。テストが `cases` を読めるように `registerTesting` の戻り値に `cases` を足し、差し替えた vscode の API に `Range` を、テストのホストにパッケージを作り直して決め直す `createPackage` を足した。
  - `a discovery that a re-rooting outlasts gives its tests to the root that was added back meanwhile`: `top/a` の発見を止めておき、パッケージを消して（`top/a` が外れる）作り直し（`top/a` が新しい項目で戻る）、戻った根の `discover` も動いている発見を待たせてから、テスト `adds` を 1 件返して続ける。今の項目にそのテストがあって `error` がなく、`cases` はその 1 件だけで、どちらの項目も `busy` でない。
  - `a discovery whose root a re-rooting removed meanwhile records no tests for it`: パッケージを消して `top/a` が外れたまま続ける。`cases` は空。
  - `d2902e31`（戻り値に `cases` を足しただけ）では、1 つ目が `actual: []`・`expected: [ 'adds' ]`、2 つ目が外れた根の `cases` の 1 件で落ちる（`unit-before.log`）。

#### 4 回目の対応の検証

- `cd vsc && npm run test:unit && npm run check-types && npm run lint`: 単体テストは 40 件すべて通過（前の 38 件に 2 件を足した）、型検査と lint はエラーなし（`unit-after.log`）。
- `reroot.test.ts` を続けて動かすと、足した 2 つは毎回通ったが、ほかの作業で混んだ機械（load average 20〜100）で、既存の `a client that starts while Restart Language Server stops the others stays tracked, so no server runs without one` がまれに落ちた（この版で 439 回中 5 回）。
  `d2902e31` でも 4 つ並べて動かすと 200 回中 1 回落ち（`z-base-fail-3-43.log`）、この版で足した 2 つを飛ばしても落ちる（`x-skip-fail-49.log`）ので、この対応とは関係しない。**Restart Language Server** は開き直しを待たずに返り、テストはその後の `settle()`（`setImmediate` 20 回）のうちに、開き直した `top/a/X.tz` の `documentHasProject`（`fs.stat`）が終わると見込んでいる。混んだ機械ではそれが間に合わない、テストの待ち方の揺れで、この対応では変えていない。
- `cd vsc && node ../scripts/slot.mjs -- npm test`（`ELECTRON_RUN_AS_NODE` と `VSCODE_*` を外す）: 16 の `VS Code:` 行と `All Tsuzuri VS Code integration checks passed.`（`integration.log`）。サーバーが要求を断る `document directory does not exist` は、前と同じく 1 回出る。
- `cargo fmt --all -- --check`（差分なし）、`node scripts/check-guide-lite.mjs`（88 件、0 件の失敗）、`git diff --check`。
- 言語リファレンスのページと Rust のコードは変えていないので、`check-docs.mjs` と `cargo nextest` は動かしていない。G40 より前も、発見のあいだにワークスペースのフォルダーを外して足し直せば起きえたが、狭い場面で、G40 の決め直しの説明（CHANGELOG の `## Unreleased`）が書く振る舞いに合わせる修正なので、CHANGELOG には書いていない。

### 変更したファイル

`vsc/src/core.ts`、`vsc/src/extension.ts`、`vsc/src/workflow.ts`、`vsc/src/testing.ts`、`vsc/src/test/core.test.ts`、`vsc/src/test/extension.test.ts`、`vsc/src/test/reroot.test.ts`、`vsc/scripts/test-extension.mjs`、`vsc/package.json`（`test:unit` に `reroot.test.js` を足した）、`vsc/README.md`、`vsc/Development.md`、`vsc/CHANGELOG.md`、`docs/architecture.md`、`_tsuzuri/language-reference/compiler/usage.md`、`_features/README.md`、`_features/G39-stale-editor-diagnostics.md`（リンクだけ）、このチケット。
