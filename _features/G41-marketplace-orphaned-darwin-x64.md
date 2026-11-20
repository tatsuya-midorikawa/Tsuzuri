# G41: VS Code で Tsuzuri 拡張機能が「Mac Silicon プラットフォームの Visual Studio Code では使用できません」となりインストールできない（Marketplace に残った 0.1.0・0.1.1 の `darwin-x64` が `IncludeLatestVersionOnly` に残り続け、クールダウン付きギャラリーで `allTargetPlatforms` が `["darwin-x64"]` だけになる）

| 項目 | 内容 |
| --- | --- |
| ID | G41 |
| 優先度 | P1（新しい版を公開した後のクールダウン期間中、`extensions.gallery.serviceUrl` で公開直後の版を留保する Marketplace プロキシを使う環境の全対応プラットフォーム（`darwin-arm64` を含む 5 環境すべて）で、拡張機能ビューの検索結果から Tsuzuri をインストールできなくなる。公開 Marketplace でも対象外の Intel Mac 向けに古い 0.1.1 が広告され続ける） |
| 規模 | S（Marketplace 管理画面での旧版 0.1.0・0.1.1 の削除、公開検証スクリプト `verify-release.mjs` と単体テスト、公開スキルと `Publishing.md` の更新） |
| 依存 | G14（done） |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-20（利用者の報告「VSCode で Tsuzuri 拡張機能をインストールしようとしたが `'Tsuzuri' 拡張機能は Mac Silicon プラットフォームの Visual Studio Code では使用できません` と表示されインストールできなかった」を受けて原因を特定し起票。HEAD `295ffcb4`（拡張 0.1.8）、Apple Silicon macOS 上の VS Code 1.147.0 で再現・確認） |
| 承認 | 要（Marketplace 上の旧版 `0.1.0`・`0.1.1` の削除は不可逆であり、現行の `.claude/skills/vscode-publish/SKILL.md` が「古い Intel Mac 版なども勝手に削除しない」と定めているため、削除操作の実施前に利用者の承認を要する。言語・コンパイラ・拡張本体のコードと既存の 0.1.2〜0.1.8 の VSIX は変更しない） |
| 改善する劣位 | 不具合: (1) 新版公開後のクールダウン期間中、公開直後の版を留保する Marketplace プロキシ経由の VS Code（Apple Silicon Mac の `darwin-arm64` を含む全 5 対応環境）で、拡張機能ビューの検索結果と `code --install-extension tmidorikawa.tsuzuri` が `0.1.1@darwin-x64` のみを返し、インストールボタンが無効化されて `'Tsuzuri' 拡張機能は Mac Silicon プラットフォームの Visual Studio Code では使用できません` と表示される。(2) 公式の公開 Marketplace でも `IncludeLatestVersionOnly` と `/latest` に廃止済みの `0.1.1@darwin-x64` が残り続け、対象外の Intel Mac に古い 0.1.1 が配布可能に見えてしまう。(3) `verify-release.mjs` が指定バージョンの 5 target しか見ず、過去版に残った対象外 `targetPlatform` や `IncludeLatestVersionOnly` の汚染を検出できない |
| 手本にする既存実装 | `.claude/skills/vscode-publish/scripts/verify-release.mjs`（`verifyMarketplace` と `marketplace` サブコマンド）、`.claude/skills/vscode-publish/scripts/verify-release.test.mjs`、`tests/platforms.mjs`（5 対応ホスト `hosts` の照合） |
| 主な影響ファイル | `.claude/skills/vscode-publish/scripts/verify-release.mjs`, `.claude/skills/vscode-publish/scripts/verify-release.test.mjs`, `.claude/skills/vscode-publish/SKILL.md`, `vsc/Publishing.md`, `_features/README.md` |

## 実装者向け早見

1. **何が起きているか**: `0.1.0`（2026-09-29）と `0.1.1`（2026-09-30）で公開した `darwin-x64` パッケージが、`0.1.2`（2026-10-21、コミット `d719cf68`・`f105fb12`）で 5 環境限定へ移行した後も Visual Studio Marketplace 上に残っている。Marketplace の `IncludeLatestVersionOnly`（フラグ `0x200`）は「これまでに公開された各 `targetPlatform` の最新版」を返すため、現行最新 `0.1.8` の 5 環境に加えて `0.1.1@darwin-x64` の計 6 件を常に返す。
2. **なぜ Mac Silicon でインストール不能になるか**: 組織ポリシー等で `extensions.gallery.serviceUrl`（`ExtensionGalleryServiceUrl`）に公開直後の版を一定期間留保（クールダウン／検疫）する Marketplace プロキシが設定されていると、`IncludeLatestVersionOnly` の応答からクールダウン期間内の `0.1.8`（および `0.1.7`）の 5 環境がすべて除外される。通常の拡張機能なら `versions` が空になってプロキシが旧版（`0.1.6` の 5 環境）へフォールバックするが、`tmidorikawa.tsuzuri` は古い `0.1.1@darwin-x64`（2026-09-30）だけがクールダウン期間外として 1 件生き残るためフォールバックが起きず、`versions: [0.1.1@darwin-x64]` だけが返る。VS Code の `ExtensionGalleryService.queryGalleryExtensions` は `compatible: false` で検索し、pre-release のない拡張では `IncludeVersions` の再照会を行わないため、`allTargetPlatforms` が `["darwin-x64"]` に固定され、`darwin-arm64`（Mac Silicon）を含む全 5 対応環境でプラットフォーム非互換と判定される。
3. **どう直すか**: (1) 利用者承認のもと、Visual Studio Marketplace の Publisher 管理画面の **More Actions > Reports > Manage > Delete this version** で旧版 `0.1.0` と `0.1.1` の両方を削除する（**拡張全体を削除し ID を永久ロックする `vsce unpublish` や `More Actions > Remove` は絶対に使わない**）。(2) `verify-release.mjs` の `verifyMarketplace` を拡張し、全公開バージョンの `targetPlatform` に 5 対応ホスト以外（`darwin-x64` や `undefined` の universal など）が含まれないことと、`IncludeLatestVersionOnly` の公開照会でも指定バージョンの 5 対応ホストだけが返ることを検証する。(3) `SKILL.md` と `vsc/Publishing.md` を更新する。

## 目的

Visual Studio Marketplace 上の `tmidorikawa.tsuzuri` から廃止済みの `darwin-x64` 版（`0.1.0`・`0.1.1`）を除去し、公開 Marketplace でもクールダウン付き Marketplace プロキシ経由でも、対応する 5 環境（`darwin-arm64`、`linux-arm64`、`linux-x64`、`win32-arm64`、`win32-x64`）すべてで常に正しく検索・インストールできるようにする。あわせて公開検証スクリプトで孤立 `targetPlatform` の混入・残存を再発防止する。

## 着手条件と停止条件

### 着手条件

- G14 が `_features/README.md` で done であること（HEAD で done）。
- Marketplace 上の旧版 `0.1.0`・`0.1.1` を削除することについて、利用者（Publisher `tmidorikawa` の所有者）の明示的な承認を得ていること（D1）。
- 着手前に `vsce show tmidorikawa.tsuzuri --json` で公開中の全バージョンと `targetPlatform` の一覧を記録し、`0.1.2` 以降の安定版が 5 環境すべて `Validated`（`flags & 1 === 1`）で揃っていることを確認していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- Marketplace の管理画面（**More Actions > Reports > Manage > Delete this version**）で `0.1.0` または `0.1.1` を削除できない、または削除後に `0.1.2`〜最新版の 5 環境のいずれかが欠ける兆候がある。**いかなる場合も `vsce unpublish` や管理画面の `More Actions > Remove`（拡張全体の削除）を実行してはならない。**
- `vsc/Publishing.md` の変更後に `node tests/platforms.mjs` が失敗する（対応プラットフォーム表の正規表現に余分な行が一致した場合など）。

## 現状（2026-11-20、`main` の `295ffcb4` で確認）

### 1. Marketplace 上の公開履歴と孤立した `darwin-x64`

`vsce show tmidorikawa.tsuzuri --json`（`IncludeVersions`）で確認した現在の公開バージョンは次の 47 エントリである（すべて `engines.vscode: "^1.103.0"`、`flags: 1` = `Validated`）。

| バージョン | 公開日時（UTC） | 公開されている `targetPlatform` |
| --- | --- | --- |
| `0.1.8` | 2026-11-19 21:53〜22:12 | `darwin-arm64`, `linux-arm64`, `linux-x64`, `win32-arm64`, `win32-x64`（5 環境） |
| `0.1.7` | 2026-11-18 23:09〜23:17 | 同 5 環境 |
| `0.1.6` | 2026-11-01 12:32〜12:43 | 同 5 環境 |
| `0.1.5` | 2026-10-25 01:24〜01:32 | 同 5 環境 |
| `0.1.4` | 2026-10-23 15:36〜16:01 | 同 5 環境 |
| `0.1.3` | 2026-10-21 23:54〜2026-10-22 00:05 | 同 5 環境 |
| `0.1.2` | 2026-10-21 05:53〜06:02 | 同 5 環境 |
| `0.1.1` | 2026-09-30 05:14〜05:24 | 同 5 環境 **+ `darwin-x64`**（6 環境） |
| `0.1.0` | 2026-09-29 23:11〜23:25 | 同 5 環境 **+ `darwin-x64`**（6 環境） |

2026-10-21 のコミット `d719cf68`（"Restrict native support and CI to five platforms"）と `f105fb12`（"Enforce five-platform support throughout the VS Code extension"）で Intel Mac（`darwin-x64`）を廃止し、以降の `0.1.2`〜`0.1.8` は 5 環境のみを公開した。その際、コミット `b22c70b6` で追加した `.claude/skills/vscode-publish/SKILL.md` で「古い Intel Mac 版なども勝手に削除しない」とし、`verify-release.mjs` も指定バージョンの 5 環境だけを検査していたため、`0.1.0`・`0.1.1` の `darwin-x64` が Marketplace に残ったままになっていた。

### 2. Marketplace の `IncludeLatestVersionOnly` の仕様

VS Code が拡張機能ビューの検索（`ExtensionGalleryService.query`）や UUID 未指定の拡張解決（`code --install-extension tmidorikawa.tsuzuri`）で使う `POST /_apis/public/gallery/extensionquery` には、`IncludeLatestVersionOnly`（`0x200` = `512`）フラグが付く。

Visual Studio Marketplace は `IncludeLatestVersionOnly` に対し、「拡張全体の単一の最新バージョン」ではなく**「その拡張で過去に公開されたことがある各 `targetPlatform` ごとの最新バージョン」**を返す。そのため、公式の `https://marketplace.visualstudio.com/_apis/public/gallery/extensionquery`（および `GET /_apis/public/gallery/vscode/tmidorikawa/tsuzuri/latest` と `https://www.vscode-unpkg.net/_gallery/tmidorikawa/tsuzuri/latest`）は現在も次の **6 件**を返す。

- `0.1.8@win32-arm64`（2026-11-19）
- `0.1.8@win32-x64`（2026-11-19）
- `0.1.8@linux-x64`（2026-11-19）
- `0.1.8@linux-arm64`（2026-11-19）
- `0.1.8@darwin-arm64`（2026-11-19）
- **`0.1.1@darwin-x64`（2026-09-30）**

### 3. エラー発生までの完全な経路（再現確認済み）

Apple Silicon Mac（`uname -m` = `arm64`、VS Code 1.147.0 arm64 ネイティブバイナリ）で次の経路により再現することを確認した。

1. **クールダウン付き Marketplace プロキシによるフィルタリング**:
   管理ポリシー `ExtensionGalleryServiceUrl`（設定キー `extensions.gallery.serviceUrl`）で、公開後一定期間（クールダウン／検疫期間）の新しい版を除外する Marketplace プロキシが設定されている環境では、`IncludeVersions`（フラグ `0x1`）や `/latest` に対する応答からは `0.1.7`（2026-11-18）と `0.1.8`（2026-11-19）が除外され、`0.1.6`（2026-11-01）以下の 37 件（`0.1.6@darwin-arm64` を含む）が返る。
   しかし `IncludeLatestVersionOnly`（フラグ `0x200`）の応答では:
   - すべての `targetPlatform` を毎回同時に公開している通常の拡張機能は、上流の 6 件すべてがクールダウン期間内になるためフィルタ後の `versions` が空（`[]`）になり、プロキシが `IncludeVersions` へフォールバックしてクールダウン通過済みの最新版（全プラットフォーム分）を返す。
   - 一方 `tmidorikawa.tsuzuri` は、上流の 6 件のうち `0.1.8` の 5 件（2026-11-19）だけがクールダウンで除外され、**2026-09-30 公開の `0.1.1@darwin-x64` だけがクールダウン通過済みとして 1 件残る**。`versions` が空にならないためプロキシのフォールバックが発動せず、`versions: [{ version: "0.1.1", targetPlatform: "darwin-x64", flags: "validated" }]` の **1 件だけ**が VS Code に返る。
   （同じ端末でも `ExtensionGalleryServiceUrl` ポリシーが適用されない VS Code Insiders 1.148.0-insider は公開 Marketplace を直接参照するため、`0.1.8@darwin-arm64` のインストールに成功していた。）

2. **VS Code 側の `ExtensionGalleryService` が `IncludeVersions` を再照会しない理由**:
   VS Code（`vs/platform/extensionManagement/common/extensionGalleryService.ts`）の検索処理 `query()` は、非互換な拡張も一覧に表示するため `compatible: false` で `queryGalleryExtensions` を呼ぶ。
   - `getAllTargetPlatforms(rawGalleryExtension)` が `versions` から `allTargetPlatforms = ["darwin-x64"]` を作る。
   - `getValidRawGalleryExtensionVersion` は `darwin-arm64` に一致する版を見つけられないが、末尾の `return version || compatible ? null : versions.find(v => !isBlocked(v)) ?? null` により、`compatible === false` のため `null` ではなく **`0.1.1@darwin-x64` を返す**。
   - `queryGalleryExtensions` が `IncludeLatestVersionOnly` を外して `IncludeVersions` で再照会する条件は `!extension || (extension.properties.isPreReleaseVersion && (!includePreRelease || !extension.hasReleaseVersion)) || (!extension.properties.isPreReleaseVersion && extension.properties.targetPlatform !== options.targetPlatform && extension.hasPreReleaseVersion)` であり、**`hasPreReleaseVersion` が `true` の拡張でしか `targetPlatform` 不一致時の再照会を行わない**。`tmidorikawa.tsuzuri` には pre-release 版がない（`hasPreReleaseVersion === false`）ため再照会は行われず、`version = "0.1.1"`・`allTargetPlatforms = ["darwin-x64"]` の `IGalleryExtension` が確定する。

3. **インストール拒否メッセージの表示**:
   拡張機能エディター（`拡張機能: Tsuzuri`）およびインストール処理は `AbstractExtensionManagementService.canInstall` → `isExtensionPlatformCompatible` を呼び、`getTargetPlatform()`（`"darwin-arm64"`、表示名 `"Mac Silicon"`）が `allTargetPlatforms`（`["darwin-x64"]`）に含まれるかを検査する。一致しないため **インストール** ボタンを無効化し、`incompatible platform` メッセージとして次を表示する:
   ```text
   'Tsuzuri' 拡張機能は Mac Silicon プラットフォームの Visual Studio Code では使用できません。 理由をご確認ください
   ```
   同じ理由で、クールダウン期間中は `darwin-arm64` だけでなく `win32-x64`・`win32-arm64`・`linux-x64`・`linux-arm64` でも同様にインストール不能になる（対象外であるはずの `darwin-x64` だけが `0.1.1` をインストールできてしまう）。

### 4. 修正完了までの暫定回避策（利用者向け）

本チケットの修正が反映される前でも、バージョン番号を明示した CLI インストールであれば、VS Code の `getExtensionsUsingQueryApi` が `IncludeLatestVersionOnly` ではなく `IncludeVersions`（`0x1`）で照会するため、クールダウン通過済みの `0.1.6@darwin-arm64`（または手元でビルド・取得した VSIX）をインストールできる:

```sh
# クールダウン通過済みの版をバージョン指定で導入する場合（IncludeVersions が使われる）
code --install-extension tmidorikawa.tsuzuri@0.1.6

# または検証済み VSIX ファイルを直接指定する場合
code --install-extension /path/to/tsuzuri-0.1.8-darwin-arm64.vsix
```

（バージョンなしの `code --install-extension tmidorikawa.tsuzuri` は `uuid` 未指定のため `IncludeLatestVersionOnly` 経路に入り、検索画面と同じ理由で失敗する。）

## 仕様（直した後の振る舞い）

1. Visual Studio Marketplace 上の `tmidorikawa.tsuzuri` に存在するすべての公開バージョン（`metadata.versions`）の `targetPlatform` が、対応する 5 環境（`darwin-arm64`、`linux-arm64`、`linux-x64`、`win32-arm64`、`win32-x64`）のいずれかである。`darwin-x64` や `targetPlatform` 未設定（universal）のエントリは 1 件も存在しない（D1）。
2. 公式 Marketplace の `POST /_apis/public/gallery/extensionquery`（`IncludeLatestVersionOnly`）および `GET /_apis/public/gallery/vscode/tmidorikawa/tsuzuri/latest` が返す `versions` に `darwin-x64` が含まれず、最新バージョンの 5 環境のみが返る。
   これにより:
   - 公開 Marketplace では Intel Mac（`darwin-x64`）に対して非対応と正しく表示され、古い `0.1.1` がインストールされなくなる。
   - クールダウン付き Marketplace プロキシでは、最新版がクールダウン期間内にある間 `IncludeLatestVersionOnly` の全 5 件が一括で除外されて空になるため、プロキシのフォールバックが正常に働いてクールダウン通過済みの最新 5 環境（例: `0.1.6` の 5 環境）が返り、`darwin-arm64`（Mac Silicon）を含む全 5 対応環境で検索・インストールが成功する。
3. `.claude/skills/vscode-publish/scripts/verify-release.mjs` の `verifyMarketplace(version, metadata, latestMetadata)` は次をすべて検証する（D2）:
   - 指定した `version` のエントリが 5 対応ホストと過不足なく一致し、すべて `ExtensionVersionFlags.Validated` を持つこと（既存どおり）。
   - `metadata.versions` 内の**すべての版**について、`targetPlatform` が 5 対応ホストのいずれかであること（過去版に `darwin-x64`・`universal`・未知の `targetPlatform` が 1 件でもあれば失敗する）。
   - `latestMetadata`（`IncludeLatestVersionOnly` 照会の結果。CLI の `marketplace VERSION` 実行時は公開 Gallery API から取得して渡す）が与えられた場合、その `versions` の全エントリが `version` かつ 5 対応ホストと一致し、すべて `Validated` であること。

## 設計

| ファイル / 対象 | 場所 | 変更 |
| --- | --- | --- |
| Visual Studio Marketplace | Publisher `tmidorikawa` > `Tsuzuri` > **More Actions > Reports > Manage > Delete this version** | 旧版 `0.1.0` と `0.1.1` を削除する（`0.1.2`〜`0.1.8` は保持）。**`More Actions > Remove` や `vsce unpublish` は使わない** |
| `.claude/skills/vscode-publish/scripts/verify-release.mjs` | `verifyMarketplace`、CLI の `marketplace` 分岐 | `metadata.versions` 全件の `targetPlatform` が `hosts` に含まれることを検査する。さらに公開 Gallery API（`POST https://marketplace.visualstudio.com/_apis/public/gallery/extensionquery`、`IncludeLatestVersionOnly \| IncludeVersionProperties \| ExcludeNonValidated`）を認証なしで照会する補助関数を足し、`IncludeLatestVersionOnly` の応答にも対象外 target や旧版の取り残しがないことを検査する |
| `.claude/skills/vscode-publish/scripts/verify-release.test.mjs` | `catalog visibility is not completion...` テスト | 旧フィクスチャにあった `{ version: '0.1.1', targetPlatform: 'darwin-x64', flags: 1 }` の許容を廃止し、(a) 5 対応ホストのみの旧版（例: `0.1.6@darwin-arm64` 等）が混在する場合は成功すること、(b) 別バージョンであっても `darwin-x64` や `targetPlatform: undefined`（universal）が残っていれば失敗すること、(c) `IncludeLatestVersionOnly` の応答に旧版の別 target が混ざっていれば失敗することを単体テストする |
| `.claude/skills/vscode-publish/SKILL.md` | 「実行範囲と禁止事項」・「6. Marketplace の検証完了まで確認する」 | 「古い Intel Mac 版なども勝手に削除しない」という旧記述を、「公開カタログの全バージョンが 5 対応 target のみで構成される状態を保つ（拡張全体を削除する `vsce unpublish` や `More Actions > Remove`、既存版の無断削除・上書きは禁止）」へ改め、`verify-release.mjs marketplace` が全版の target と `IncludeLatestVersionOnly` も検査することを明記する |
| `vsc/Publishing.md` | §2・§10・§11 | 廃止した `targetPlatform` の旧版を Marketplace に残すと `IncludeLatestVersionOnly` とクールダウン付きプロキシで全対応環境のインストールを壊すこと、特定旧版の削除は **More Actions > Reports > Manage > Delete this version** を使い **More Actions > Remove**／`vsce unpublish`（拡張 ID の永久ロックを伴う全削除）と混同しないこと、よくある問題に `'Tsuzuri' 拡張機能は ... プラットフォームの Visual Studio Code では使用できません` の確認項目を追加する（`tests/platforms.mjs` が読む `\| target \|` 表の行は 5 環境のまま変えない） |

## 実装手順

### 段 1: 公開検証チェッカーと単体テストの更新

- 変更: `.claude/skills/vscode-publish/scripts/verify-release.mjs`、`.claude/skills/vscode-publish/scripts/verify-release.test.mjs`。
- 内容:
  1. `verifyMarketplace(version, metadata, latestMetadata)` で、`metadata.versions` の全要素について `hosts.includes(item.targetPlatform)` を検査し、違反があれば版と `targetPlatform` を含む明確なエラーを投げる。
  2. `latestMetadata` が渡されたときは、`latestMetadata.versions`（`IncludeLatestVersionOnly` の応答）に `version` 以外の版が混ざっていないこと、および `targetPlatform` の集合が `hosts` と一致することを検査する。
  3. CLI の `marketplace VERSION` 実行時は、`vsce show tmidorikawa.tsuzuri --json` に加えて公開 Gallery API の `extensionquery`（`filterType: 7` = `tmidorikawa.tsuzuri`、`filterType: 8` = `Microsoft.VisualStudio.Code`、`flags: 0x200 | 0x10 | 0x20`）を `fetch` で取得して `verifyMarketplace` に渡す。
  4. `verify-release.test.mjs` を更新し、オフラインで全ケースを検証する。
- 確認: `node --test .claude/skills/vscode-publish/scripts/verify-release.test.mjs` が全件通ること。また削除前の Marketplace に対して `node .claude/skills/vscode-publish/scripts/verify-release.mjs marketplace 0.1.8` を実行すると、`0.1.0`・`0.1.1` の `darwin-x64` 残存を検出して非ゼロ終了することを確認する。

### 段 2: 公開スキルと公開手順書の更新

- 変更: `.claude/skills/vscode-publish/SKILL.md`、`vsc/Publishing.md`。
- 内容: 設計の表に従い、孤立 `targetPlatform` の禁止と **Reports > Manage > Delete this version**／`Remove`（`vsce unpublish`）の区別、トラブルシューティングの項目を追記する。
- 確認: `node tests/platforms.mjs` が通ること。

### 段 3: Marketplace 上の旧版 `0.1.0`・`0.1.1` の削除（要利用者承認・管理画面操作）

- 前提: 利用者の承認を確認し、`vsce show tmidorikawa.tsuzuri --json` で `0.1.2`〜最新版の全 5 環境が `Validated` であることを再確認する。
- 手順:
  1. [Visual Studio Marketplace の Publisher 管理画面](https://marketplace.visualstudio.com/manage/publishers/tmidorikawa) にサインインする。
  2. `Tsuzuri`（`tmidorikawa.tsuzuri`）のメニューから **More Actions > Reports** を開き、**Manage** タブを開く（**More Actions > Remove** や **Unpublish** は選ばない）。
  3. バージョン `0.1.1` と `0.1.0` の **両方** について **Delete this version** を実行する（片方だけ残すと、もう片方の `darwin-x64` が `IncludeLatestVersionOnly` に繰り上がって同じ不具合が続く）。
  4. 公開カタログおよび CDN（`www.vscode-unpkg.net`）・プロキシへの伝播を待ち、`node .claude/skills/vscode-publish/scripts/verify-release.mjs marketplace 0.1.8` が成功（終了コード 0）することを確認する。
  5. Apple Silicon Mac の VS Code で拡張機能ビューから `Tsuzuri` を検索し、非互換警告が出ず **インストール** ボタンからインストールできることを確認する。

### 段 4: チケット完了処理

- `_features/README.md` の G41 の状態を `done` に更新し、本チケットに `## 実装と検証（YYYY-MM-DD）` を追記して `_features/_completed/` へ移動する。
- `node scripts/check-guide-lite.mjs` と `node tests/repo_kb.mjs` が通ることを確認する。

## テスト計画

- `node --test .claude/skills/vscode-publish/scripts/verify-release.test.mjs`: オフライン単体テスト（5 ホストのみの旧版許容、`darwin-x64`／universal 残存の拒否、`IncludeLatestVersionOnly` への旧 target 混入の拒否）。
- `node tests/platforms.mjs`: 5 対応ホストの定義と `vsc/Development.md`・`vsc/Publishing.md`・CI ワークフローの一致検査。
- `node .claude/skills/vscode-publish/scripts/verify-release.mjs marketplace 0.1.8`: 段 3 の実施前には `darwin-x64` 残存で失敗し、段 3 の実施・伝播後には成功すること。
- `node tests/repo_kb.mjs` および `node scripts/check-guide-lite.mjs`: リポジトリ KB と GUIDE-LITE の整合性検査。

## ドキュメント

- `vsc/Publishing.md`、`.claude/skills/vscode-publish/SKILL.md`（段 2）。
- 言語仕様・CLI・コンパイラ・拡張機能の実行時挙動は変わらないため、`_tsuzuri/language-reference/`・`docs/language.md`・`README.md` の変更は不要。

## 受け入れ条件

- [ ] `node --test .claude/skills/vscode-publish/scripts/verify-release.test.mjs` が通り、過去版や `IncludeLatestVersionOnly` に 5 対応ホスト以外の `targetPlatform`（`darwin-x64` や universal を含む）が残っているケースを確実に拒否する。
- [ ] `node tests/platforms.mjs` が通る。
- [ ] Marketplace から `0.1.0` と `0.1.1` の `darwin-x64` が除去され、`0.1.2`〜最新版の 5 対応ホストはすべて `Validated` のまま保持されている。
- [ ] `node .claude/skills/vscode-publish/scripts/verify-release.mjs marketplace <最新版>` が成功し、`IncludeLatestVersionOnly` の公開照会でも最新版の 5 対応ホストのみが返る。
- [ ] Apple Silicon Mac（`darwin-arm64`）の VS Code で拡張機能ビューから `Tsuzuri` を検索・表示したとき、「Mac Silicon プラットフォームの Visual Studio Code では使用できません」が出ず、そのままインストールできる。

## 落とし穴

- **`vsce unpublish` と管理画面の `More Actions > Remove` は絶対に使わない**: これらは特定バージョンではなく拡張機能全体を削除する操作であり、Visual Studio Marketplace の仕様により一度削除した拡張機能名（`tmidorikawa.tsuzuri`）は元の Publisher であっても再利用できなくなる（[Publishing Extensions - Removing extensions](https://code.visualstudio.com/api/working-with-extensions/publishing-extension#removing-extensions)）。特定バージョンの削除は必ず **More Actions > Reports > Manage > Delete this version**（[Removing specific extension versions](https://code.visualstudio.com/api/working-with-extensions/publishing-extension#removing-specific-extension-versions)）を使う。
- **`0.1.1` だけでなく `0.1.0` も削除する**: `0.1.1` だけを削除すると、Marketplace の `IncludeLatestVersionOnly` が次に新しい `darwin-x64` として `0.1.0@darwin-x64`（2026-09-29）を返すようになり、クールダウン付きプロキシでの不具合が全く同じ形で残る。
- **最新版は削除できない**: Marketplace の仕様上、拡張機能の最新バージョン（現在 `0.1.8`）は **Delete this version** で削除できない。削除対象は `darwin-x64` を含む旧版 `0.1.0`・`0.1.1` のみであり、`0.1.2`〜`0.1.8` には触れない。また一度削除したバージョン番号（`0.1.0`・`0.1.1`）は再公開に使えないが、既に `0.1.8` まで進んでいるため影響はない。
- **`tests/platforms.mjs` の表検査**: `tests/platforms.mjs` は `vsc/Publishing.md` と `vsc/Development.md` 内の `/^\| \`?((?:darwin|linux|win32)-[a-z0-9]+)\`? \|/gm` に一致するすべての Markdown 表の行を抽出し、5 対応ホストと完全一致することを検査する。`vsc/Publishing.md` にトラブルシューティングや経緯を書く際、第 1 列が `darwin-x64` になる表の行を足してはならない。
- **カタログ・CDN・プロキシの伝播遅延**: 管理画面で `0.1.0`・`0.1.1` を削除した直後は、公開カタログの検索索引・`www.vscode-unpkg.net`・プロキシのキャッシュに古い応答が数分〜しばらく残ることがある。`verify-release.mjs marketplace` で公開カタログ側の反映を先に確かめ、VS Code 側は必要に応じてウィンドウの再読み込みまたはキャッシュ更新後に確認する。

## 対象外

- Intel Mac（`darwin-x64`）や universal パッケージの新規作成・再対応（コミット `d719cf68`・`f105fb12` の 5 環境方針を維持する）。
- VS Code 本体の `ExtensionGalleryService.queryGalleryExtensions`（`hasPreReleaseVersion === false` のとき `targetPlatform` 不一致でも `IncludeVersions` を再照会しない挙動）や外部プロキシ実装の変更。
- 拡張機能本体のコード変更や、この件単独のための新バージョン（`0.1.9`）の公開（既存の `0.1.2`〜`0.1.8` はすべて正しい 5 環境パッケージとして公開・検証済みであるため、`0.1.0`・`0.1.1` の削除だけで現行版・過去版ともに正常化する）。

## 決定事項

- **D1: Marketplace 上の旧版 `0.1.0`・`0.1.1` を管理画面の「Delete this version」で削除して孤立した `darwin-x64` を解消する**（要利用者承認）。
  理由: 原因の根は廃止済み `targetPlatform`（`darwin-x64`）の旧パッケージが Marketplace に残り、`IncludeLatestVersionOnly` に永続的に混入し続けることにある。`0.1.0`・`0.1.1` を削除すれば、`0.1.2`〜`0.1.8` の全バージョンと拡張 ID・統計を保ったまま、公開 Marketplace とクールダウン付きプロキシの双方で根本的に解決する。
  代案と退けた理由:
  - ダミーの `darwin-x64` VSIX や universal VSIX を毎回一緒に公開する: 5 環境のみを配布するというリポジトリ全体の不変条件（`f105fb12`、`tests/platforms.mjs`）を破り、対象外の Intel Mac 等へインストール可能なパッケージを配り続けることになるため採用しない。
  - pre-release 版を公開して VS Code 側の `hasPreReleaseVersion` 分岐（`IncludeVersions` 再照会）を誘発させる: VS Code クライアントの内部実装詳細に依存する脆い回避策であり、公開 Marketplace 上で `0.1.1@darwin-x64` が広告され続ける問題も直らないため採用しない。
- **D2: `verify-release.mjs` の `marketplace` 検査で、全公開バージョンの `targetPlatform` と `IncludeLatestVersionOnly` の応答も検証する**（既定案）。指定バージョンだけを見る検査では過去版の孤立 `targetPlatform` を見逃したため、カタログ全体の不変条件として機械的に検査する。
- **D3: `SKILL.md` と `vsc/Publishing.md` で「拡張全体の削除（`vsce unpublish`／`Remove`）の禁止」と「廃止済み `targetPlatform` をカタログに残さない原則」を明確に分けて記載する**（既定案）。
