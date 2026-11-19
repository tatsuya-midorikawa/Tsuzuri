# G40: VS Code でプロジェクトでないフォルダーを開くと、その根のクライアントが中のパッケージのファイルまで受け持つ（消えない偽のエラー、診断・ホバーの重複、このリポジトリでの `E1017`）

| 項目 | 内容 |
| --- | --- |
| ID | G40 |
| 優先度 | P2（直下に `Tsuzuri.toml` も `Main.tz` もないフォルダーの下にパッケージがあり、パッケージの外の `.tz` を開いたときに起きる。このリポジトリを開いて `std/`・`tests/` の `.tz` を開くと必ず起きる） |
| 規模 | M（拡張だけ。根の決め方、残る場合の selector と診断の絞り込み、印の変化への追従、統合テスト） |
| 依存 | G14, G16（すべて done）, (G39) |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-19（G39 の調査で見つけ、利用者の依頼で別のチケットにした。HEAD `af644794`（拡張 0.1.7）で、VS Code 1.103.2 に拡張を入れ、言語サーバーの各プロセスが受け取ったメッセージを記録する compiler の shim（`tsuzuri.compilerPath`）で再現した） |
| 承認 | 要承認: D1（印のないファイルの根の決め方を変える。言語と CLI は変えないが、そうしたファイルのモジュール名と、ディレクトリをまたぐ参照の解決が変わる）。D2〜D6 は既定案 |
| 改善する劣位 | 不具合: (1) フォルダーの根のクライアントが、中のパッケージのファイルにマニフェストを読まない解析で偽のエラーを出し、直しても消えない。(2) 同じ診断・ホバー・シンボル・補完が 2 つずつ出る。(3) 根の外の `.tz`（メモ、試し書き）のホバー・定義・シンボル・色分けが、中のパッケージの解析の失敗のせいで何も返さない。(4) このリポジトリでは根のクライアントの読み込みが毎回 `E1017` で失敗し、それがディレクトリの URI に付く。テストの発見とタスクも、ビルドできないフォルダーの根を対象にする |
| 手本にする既存実装 | `vsc/src/core.ts` の `projectRoot`（`fs` で上へ印を探す。単体テストは `vsc/src/test/core.test.ts` の `project root respects nested projects and source subdirectories`）、`vsc/src/extension.ts` の `open`（根ごとの `LanguageClient`、documentSelector の作り方と escape）、`vsc/scripts/test-extension.mjs`（統合テストのワークスペースの作り方）、vscode-languageclient の `middleware.handleDiagnostics` |
| 主な影響ファイル | `vsc/src/core.ts`, `vsc/src/extension.ts`, `vsc/src/workflow.ts`, `vsc/src/testing.ts`, `vsc/src/test/core.test.ts`, `vsc/src/test/extension.test.ts`, `vsc/scripts/test-extension.mjs`, `vsc/README.md`, `vsc/CHANGELOG.md`, `docs/architecture.md`, `_tsuzuri/language-reference/compiler/usage.md`, `_features/README.md` |

## 目的

1 つの `.tz` ファイルを、1 つのクライアント（言語サーバーのプロセス）だけが受け持つようにする。
今は、プロジェクトでないフォルダーの根のクライアントの documentSelector（`<根>/**/*.{tz,tt,tc}`）が、中のパッケージのファイルにも当たる。
そのため、パッケージのファイルを 2 つの言語サーバーが別々の根で解析し、片方はマニフェストを読まないので偽のエラーを出す。

1. 印（`Tsuzuri.toml` か `Main.tz`）のないファイルの根を、中に別のプロジェクトを含まないディレクトリにする（D1）。
2. それでも根の下にプロジェクトが残る場合（印のないファイルが、パッケージを含むディレクトリの直下にある）は、そのクライアントが直下のファイルだけを受け持つ（D2）。
3. 印が増えたり消えたりしたら、根を決め直す（D4）。

## 着手条件と停止条件

### 着手条件

- G14・G16 が done であること。G39 の Phase 1（D1: テストの発見が診断を出さない）が done であることが望ましい（下の落とし穴を参照）。
- D1 の承認が記録されていること（`要承認`）。
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
      印を含むかどうかは、モジュールの発見と同じく `.` で始まる名前を飛ばして数え、1024 個のディレクトリか 16 段を超えたら「含む」とみなす（その根の読み込みは `E1017` で失敗するため）。
   4. ファイルのディレクトリ自体が下に印を含むなら（印のないファイルが、パッケージを含むディレクトリの直下にある）、そのディレクトリ。これを「残る根」と呼ぶ。
   どのワークスペースのフォルダーにも入らないファイルは、今と同じくそのディレクトリ。
2. 残る根のクライアントは、根の直下のファイルだけを受け持つ。documentSelector は `<根>/*.{tz,tt,tc}`（`**` でなく `*`）にする（D2）。
   そのサーバーは根を再帰的に読むので、直下でないファイルの診断を送ってくる。`middleware.handleDiagnostics` で根の直下でない URI を空にし、ワークスペースのシンボルの結果からも外す。
3. 印を含まない根のクライアントは、今と同じく `<根>/**/*.{tz,tt,tc}` を受け持つ。下に印がないので、ほかのクライアントと重ならない。
4. テストの根とタスクの `project` も、1 の根に従う。
5. ワークスペースのフォルダーの下で `Tsuzuri.toml` か `Main.tz` が作られるか消えたら、1 の結果の cache を捨て、根が変わる文書のクライアントを起動し直し、どの文書の根でもなくなったテストの根を Testing ビューから外す（D4）。
6. 次は変えない: 印のあるプロジェクトの根、プロジェクトの中のプロジェクト（再現 (c)・(c2)）、`tsuzuri.projectPath`、マルチルートのワークスペース、言語サーバーとモジュールの発見。

### 変わること（利用者に見える）

- 再現 (a): クライアントは `ws/scratch` と `ws/app` になる。偽の `E1002` と重複が消え、`scratch/Try.tz` のホバーが返る。テストの根に `ws` が入らない。
- 再現 (b): クライアントは `examples/point`・`std`・`tests/conformance/ABI` などになる。ディレクトリの URI の `E1017` は出なくなる。
  代わりに、`std/` を根にした解析は std のモジュール名を予約名として `E1011` を 50 件出し、`tests/conformance/ABI` の fixture は 11 件の `E1011` を出す（`target/scratch/g40-pb08/g40/proto/logs/roots.out`。std の開発を言語サーバーで扱う仕組みは対象外）。
- パッケージを含むフォルダーの下で、印のないファイルがディレクトリをまたいで参照し合っていると（`x/X.tz` が `Root::y::Y.f()` を呼ぶ）、今は解決するが、直した後は `E1002` になる。
  2 つのディレクトリが別の根になるため（`verify-g40-routing/e2e/runs/xdir` と `xdir3`）。`tsuzuri check x/X.tz` も同じ `E1002` を出す。パッケージを含まないフォルダーでは、今と同じく 1 つの根なので変わらない。
- そうしたファイルのモジュール名は、根からの相対パスで決まるので変わる（`Ws::scratch::Try` が `Scratch::Try` になる）。

## 設計

| ファイル | 関数・場所 | 変更 |
| --- | --- | --- |
| `vsc/src/core.ts` | `projectRoot`（33〜50 行） | 仕様 1 の 3・4 を足す。ディレクトリごとに「下に印を含むか」を幅優先で調べ（印を見つけたらそこで止める、1024 個か 16 段で打ち切る、`.` で始まる名前とシンボリックリンクのディレクトリは辿らない）、結果をディレクトリごとに cache する。純粋な部分は単体テストで確かめる |
| `vsc/src/core.ts` | 新しい関数 | 根が「残る根」かどうか（根の下に印がある）と、パスが根の直下かどうかの判定 |
| `vsc/src/extension.ts` | `open`（26〜52 行） | 残る根には documentSelector `<根>/*.{tz,tt,tc}` と、直下でない URI を空にする `middleware.handleDiagnostics`、`middleware.provideWorkspaceSymbols` の絞り込みを付ける。今の escape（`:42`）をそのまま使う |
| `vsc/src/extension.ts` | 新しい watcher | ワークスペースのフォルダーごとに `**/{Tsuzuri.toml,Main.tz}` の作成・削除を見て、cache を捨て、根が変わる文書のクライアントを起動し直す（今の再起動のコマンドは全部を止めるので、根が変わるクライアントだけにする） |
| `vsc/src/testing.ts` | `refreshRoots`（83〜96 行）と新しい購読 | 印の変化のとき、どの文書の根でもなくなったテストの根を外す（今は外さない。`:115-124` はワークスペースのフォルダーの削除だけ） |
| `vsc/src/workflow.ts` | `projectFor`（7〜21 行） | `projectRoot` の cache を使う。`projectFor` は watcher の事象ごと（`vsc/src/testing.ts:98-111`）、`refreshRoots` の開いた文書ごと、コマンドとタスクから呼ばれるので、毎回走査しない |

`projectRoot` の走査は、このリポジトリでは 1 回に約 1025 回の `readdir`（66〜126 ms）になる（`target/scratch/g40-pb08/verify-g40-routing/proto/hma.out`。深さ優先だと `vsc/toolchain` で上限に達する）。
幅優先で印を先に見つければ早く止まり、cache で 2 回目からは走査しない。

## 実装手順

各段で、先にテストを書き、HEAD で落ちることを確かめてから直す。

### 段 1: 根の決め方

- 変更: `vsc/src/core.ts` の `projectRoot` と cache。
- 確認: `vsc/src/test/core.test.ts` の `project root respects nested projects and source subdirectories` に、`root/app/Main.tz` を残したまま次を足す。
  - `projectRoot(<root>/scratch/Try.tz, root)` が `<root>/scratch`（HEAD は `root` なので落ちる）。
  - `projectRoot(<root>/Library.tz, root)` は `root` のまま（残る根。HEAD でも通る）。
  - 印のない別の一時ディレクトリで、`projectRoot(<r>/a/X.tz, r)` は `r` のまま（パッケージを含まないフォルダーは変わらない。HEAD でも通る）。
  - 1024 個を超える印のないディレクトリの木は「含む」とみなす。存在しないディレクトリの未保存のファイル（`readdir` が `ENOENT`）でも落ちない。
  - `cd vsc && npm run test:unit`。

### 段 2: 残る根の selector と診断の絞り込み

- 変更: `vsc/src/extension.ts` の `open`。
- 確認: 統合テスト（段 4）の 3。

### 段 3: 印の変化への追従

- 変更: `vsc/src/extension.ts`、`vsc/src/testing.ts`、`vsc/src/workflow.ts`。
- 確認: 統合テストで、`scratch/` に `Main.tz` を作ると `scratch` のクライアントが起動し直し、テストの根が更新され、消すと戻る。

### 段 4: 統合テスト

- 変更: `vsc/scripts/test-extension.mjs` に 3 つ目のワークスペースのフォルダー `loose/` を足す。中身は `scratch/Try.tz`（`fn t() -> i64 { 1 }`）、`App/Tsuzuri.toml`（namespace `App`）、`App/Main.tz`（`namespace App` と `App::Util.value()` の呼び出し）、ヘッダーのない `App/Util.tz`、自分のエラーを持つ `Notes.tz`（`fn n() -> i64 { true }`）。
  パッケージのディレクトリを `App`（大文字）にすると、1 回の refresh で `Notes.tz` より先に送られる（パスはバイト順。`src/driver.rs:1839`）。
- `vsc/src/test/extension.test.ts` に足す（既存の `api.clients.size === 2` を待つ検査（`:59`）の後に置く）。文書は `vscode.commands.executeCommand('vscode.open', uri)` で開き、診断は source `tsuzuri` に絞る（G39 の写しと混ぜない）。
  1. `scratch/Try.tz` を開き、`t` のホバーが返るまで待つ（HEAD は返らずに時間切れ）。`api.clients.has('<loose>/scratch')` が真、`api.clients.has('<loose>')` が偽。`App/Main.tz` に source `tsuzuri` の `E1002` がない（HEAD は開く前から出る）。
  2. `App/Main.tz` を開き、`App` のクライアントのホバーを待つ。source `tsuzuri` の診断がなく、ホバーとシンボルがそれぞれ 1 つ。
  3. 残る根: `Notes.tz` を開き、その `E1003` を待つ。`App/Main.tz` には source `tsuzuri` の `E1002` がなく、ホバーとシンボルは 1 つのまま（HEAD は `E1002` が出る）。
- `Util.tz` にヘッダーがある変種で、ホバーとシンボルが 1 つずつであること（HEAD は 2 つずつ）。
- `cd vsc && npm test`（環境変数を外し、`node scripts/slot.mjs --` を通す）。

### 段 5: 文書

- `vsc/README.md` の設定の表の `tsuzuri.projectPath`（「空なら、開いたファイルから最も近い Main.tz か Tsuzuri.toml を探す」）に、見つからないときの根（ほかのプロジェクトを含まない最も上のフォルダー）を書く。
  「1 つのフォルダーに複数のプロジェクトを入れる場合は…」の段落に、パッケージと同じフォルダーの直下にある印のないファイルは、`tsuzuri check` と同じくパッケージを含むフォルダーとして読まれ、パッケージのファイルはそれぞれのクライアントが表示することを書く。
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

- `vsc/README.md`、`vsc/CHANGELOG.md`、`docs/architecture.md`、`_tsuzuri/language-reference/compiler/usage.md`（段 5）。
- 完了したら、`_features/README.md` の G40 の行を done にし、このチケットを `_features/_completed/` へ移す。

## 受け入れ条件

- [ ] 再現 (a) で、パッケージのファイルに偽のエラーも重複も出ず、パッケージの外の印のないファイルのホバー・定義・シンボルが返る。
- [ ] 再現 (b) で、ディレクトリの URI に `E1017` が出ず、リポジトリの根のクライアントもテストの根もない。
- [ ] 残る根のクライアントは根の直下のファイルだけを受け持ち、ほかのクライアントのファイルに診断を出さない。
- [ ] `Tsuzuri.toml` か `Main.tz` を作るか消すと、根が決め直され、使われなくなったテストの根が消える。
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
- 使っていない根のクライアントを止めること（1 つ約 78 MB）。
- このリポジトリの `std/` を std として言語サーバーで扱うこと（今は `std/` を利用者のフォルダーとして読むので `E1011` になる）。

## 決定事項

- **D1: 印のないファイルの根は、下に印を含まない最も上のディレクトリ（ワークスペースのフォルダーまで）。ファイルのディレクトリ自体が印を含むなら、そのディレクトリ**（要承認）。
  理由: 根が別のプロジェクトを含まなくなるので、クライアントが重ならない。パッケージを含まないフォルダーは今と同じ 1 つの根のままで、ディレクトリをまたぐ参照とモジュール名が変わらない。
  代案と退けた理由:
  - ファイルのディレクトリを根にする（`tsuzuri check <ファイル>` と `modules.md:148` の規則）。簡単で CLI と同じだが、パッケージを含まないフォルダーでもディレクトリをまたぐ参照が解決しなくなり、`R/a.tz` と `R/sub/b.tz` で根 `R` と `R/sub` が重なる。
  - 今のまま（ワークスペースのフォルダー）にして middleware で絞る。根のサーバーはパッケージを読み込み続けるので、印のないファイルのホバーは空のまま、このリポジトリの `E1017` も残る。
  承認が要る理由: パッケージを含むフォルダーの下の印のないファイルのモジュール名と、ディレクトリをまたぐ参照の解決が変わる。このリポジトリでは `std/` の `.tz` を開くと `E1011` が 50 件出る。
- **D2: 残る根のクライアントは、selector `<根>/*.{tz,tt,tc}` で直下のファイルだけを受け持ち、診断とワークスペースのシンボルを middleware で直下に絞る**（既定案）。
- **D3: 走査は幅優先で、印を見つけたら止め、1024 個のディレクトリか 16 段で打ち切って「含む」とみなし、`.` で始まる名前とシンボリックリンクのディレクトリは辿らない**（既定案）。モジュールの発見（`src/driver.rs:1758-1844`、`modules.md:144`）と同じ数え方にするため。
- **D4: 印の作成・削除で cache を捨て、根が変わるクライアントだけを起動し直し、使われなくなったテストの根を外す**（既定案）。
- **D5: 言語サーバーとモジュールの発見は変えない**（既定案）。
- **D6: プロジェクトの中のプロジェクトは今のままにする**（既定案）。再現 (c) で外側のサーバーは依存の未保存のバッファを使い、(c2) の外側のエラーは CLI と同じなので、どちらも絞ると正しい情報を失う。
