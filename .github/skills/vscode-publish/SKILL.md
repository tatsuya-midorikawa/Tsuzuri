---
name: vscode-publish
description: "Publish or prepare a release of the Tsuzuri VS Code extension on Visual Studio Marketplace. Use for vsc の Publish・公開・リリース、拡張機能のパッチバージョン更新、5環境の VSIX 配布。Covers versioning, tagged CI artifacts, checksums, safe authentication, partial-upload recovery, and Marketplace Validated checks."
argument-hint: "[version | patch] [prepare-only]"
---

# Tsuzuri VS Code 拡張の Publish

対象は `tmidorikawa.tsuzuri`。公開先は [Marketplace](https://marketplace.visualstudio.com/items?itemName=tmidorikawa.tsuzuri)、管理先は [Publisher tmidorikawa](https://marketplace.visualstudio.com/manage/publishers/tmidorikawa)。

## 実行範囲と禁止事項

- **公開は利用者が明確に Publish・公開を依頼した場合だけ行う。** バージョン更新だけ、準備だけ、手順説明、スキル作成・変更の依頼では公開しない。認証済みであることは公開の許可ではない。
- `prepare-only` では Marketplace への書き込み、release タグの push、PR のマージを行わない。CI までの準備を明示的に頼まれた場合は、branch を ref にした `workflow_dispatch` を使い、公開の許可まで release タグは作らない。
- 明示された版を優先する。公開依頼が `patch` なら現在の版からパッチを 1 つ上げる。再開時は既存のリリースを調べ、もう一度版を上げない。pre-release は別の明示的な依頼が必要。
- 対応は `darwin-arm64`、`linux-arm64`、`linux-x64`、`win32-arm64`、`win32-x64` の 5 つだけ。Windows Arm64 のソースデバッグは未対応のまま。universal や Intel Mac の新パッケージを作らない。
- 既存の版・target を上書きせず、古い Intel Mac 版なども勝手に削除しない。`unpublish`、`--skip-duplicate`、タグの付け替え、保護ルールの回避を使わない。
- PAT・アクセストークンをチャット、コマンド引数、ファイル、Git、ログへ書かない。資格情報ストアの中身を表示しない。
- この手順を実行しても CI は自動公開しない。公開操作は、検証済み VSIX を指定する `vsce publish --packagePath` の段階だけ。

## 最初に読むもの

[作業規約](../../../AGENTS.md)、[CI の注意](../../instructions/ci.instructions.md)、[公開手順](../../../vsc/Publishing.md)、[IDE ワークフロー](../../workflows/vscode.yml) を読む。手順が変わっていたら、古い run ID や 0.1.2 の成果物を流用せず、現在の契約と照合する。

以下のコマンドはリポジトリのルートから実行する。`VERSION`、`SOURCE_SHA`、`RUN_ID`、`ARTIFACT_ID`、`HOST` などは、そのリリースについて検証した値に置換する。シェルの値はツール呼び出しをまたいで保持されないので、毎回設定するかリテラルを使う。重い処理は `node scripts/slot.mjs --` で包む。

## 1. 事前確認

1. `git status --short --branch` で利用者の変更を確認する。既存変更を戻したり、無関係な変更をコミットしたりしない。リリース元が不明、または版の変更と衝突する場合は止めて確認する。
2. main の最新 CI を基準として確認する。`vsc/package.json` の `name`・`publisher`・`version`、lockfile、CHANGELOG、公開済みの版と target を読む。
3. 既存のリリース branch・PR・`vsc-v<VERSION>` タグ・CI を調べる。公開済み、アップロード済みで検証待ち、まだ存在しない、を区別する。
4. リポジトリに固定された `vsc/node_modules/.bin/vsce` を使う。依存がなければ lockfile に従い `npm --prefix vsc ci` で復元する。グローバルの別版は使わない。
5. `vsce show tmidorikawa.tsuzuri --json` で公開状態を確認する。認証は `vsce verify-pat tmidorikawa`、または構成済み Entra ID の `vsce verify-pat tmidorikawa --azure-credential` で確認する。認証がなければ、安全なログインを利用者に依頼する。PAT は `vsce login tmidorikawa` の非表示入力へ利用者が直接入力し、チャットでは受け取らない。

## 2. 版を更新して検証する

1. 意図した main を fetch し、`release/vscode-<VERSION>` の専用 branch を作る。main へ直接コミット・push しない。
2. `npm --prefix vsc version "$VERSION" --no-git-tag-version` で版を更新する。
3. `vsc/package.json`、lockfile のルートと `packages[""]` の版が一致することを検証する。不要な整形差分は除き、publisher・name・依存関係・Cargo の版を変えない。
4. CHANGELOG に `## <VERSION> - YYYY-MM-DD` を用意し、実際の差分だけを記録する。過去のリリースを改変しない。
5. 作業規約の T0/T1/T2 を守り、拡張の `test:unit`、`check-types`、`lint`、`node tests/platforms.mjs` を実行する。開発機の VSIX 作成・再検証と `test:installed` も確認する。開発機で作ったものは公開しない。
6. VS Code がログインシェル解決で停止した場合だけ、テストの起動を `VSCODE_CLI=1 SHELL=/bin/sh` で試せる。利用者のシェル設定、テストの条件、タイムアウトを変更して成功扱いにしない。
7. 版と CHANGELOG の変更を branch にコミットする。コミットメッセージ末尾に作業規約の `Co-authored-by` を付ける。

## 3. 同じコミットから 5 環境を CI で作る

`prepare-only` はこの節より前で止める（CI の準備も依頼された場合は上の例外に従う）。

タグがまだ存在しないことをローカルと remote の両方で確かめ、リリース元の 40 桁 SHA を記録する。

```sh
git tag -a "vsc-v${VERSION:?}" -m "Tsuzuri VS Code extension $VERSION"
git push --atomic --set-upstream origin "release/vscode-$VERSION" "refs/tags/vsc-v$VERSION"
```

1. version PR を作る。タグは既存の `vscode.yml` の 5 環境マトリクスを起動する。OS・ツールチェーンの動作変更も含む場合は、作業規約に従い `ci:full` も使う。版だけのリリースで同じパッケージ CI を何度も起動しない。
2. `gh run list --workflow vscode.yml --branch "vsc-v$VERSION"` で run を探す。`event=push`、タグ名、`headSha=SOURCE_SHA` を照合し、別の main/PR の成果物と混ぜない。
3. `gh run watch "$RUN_ID" --exit-status` で待つ。**5 job 全部が success** で、実際の `Installed VSIX integration` を通ったことを確認する。赤い job は公開を止め、ログを読む。
4. GitHub API が一時的に使えない場合は、既知の run ID を使う GitHub MCP の読み取りも利用できる。タイムアウトした PR 作成などは、remote の状態を調べてから再試行する。重複 PR やタグの force push で解決しない。

## 4. CI 成果物を取得・照合する

取得する artifact 名は `tsuzuri-<HOST>` の 5 つ。`tsuzuri-cli-<HOST>` ではない。

1. run の artifact 一覧で、5 つの名前・期限・run ID・head SHA・`digest` を記録する。
2. ZIP は `target/scratch/` のリリース専用ディレクトリへ保存する。バイナリをチャットへ出力しない。`gh api .../actions/artifacts/<ID>/zip` の出力をファイルへ redirect する。slot ラッパーを使う場合も、ZIP だけがファイルへ入るようにする。
3. **展開前に ZIP 全体の SHA-256 を GitHub の artifact digest と照合する。** 中身が期待する VSIX と `.sha256` の 2 ファイルだけで、危険な展開パスがないことを確かめる。環境の ZIP ツールで、既存ファイルを上書きせず `target/dist/vsc-<VERSION>/<HOST>/` へ展開する。
4. 各 target について同梱の読み取り専用チェッカーを実行する。

```sh
node .github/skills/vscode-publish/scripts/verify-release.mjs vsix "$VERSION" "$HOST" "$VSIX"
```

チェッカーは VSIX の SHA-256 と sidecar、publisher/name/version、XML の `TargetPlatform`、ツールチェーンの OS/CPU、必須ツール、CodeLLDB の有無、リリースノートを照合する。VSCE が CHANGELOG を `extension/changelog.md` として格納することにも対応する。これは CI の実行試験や、GitHub の artifact digest の照合を代替しない。

5. 5 ファイルが同じ版・コミット・run に由来することを揃えてから公開へ進む。

## 5. PR と Marketplace に反映する

1. 公開の許可と、5 環境の CI・成果物の検証を再確認する。PR の head が検証した SHA のままで、必要なレビュー・チェックを満たす場合だけ、通常の PR マージで main に版を反映する。管理者 bypass は使わない。マージ後の tree が検証したタグと一致することを確認する。差分があれば止めて調べる。
2. `vsce verify-pat` と公開済みの版・target をもう一度確認し、公開直前にファイルの SHA-256 も再確認する。
3. 以下のように **5 ファイルを明示** して公開する。`*.vsix`、公開時の `--target`、版の再インクリメントは使わない。Entra ID を使う環境は、同じコマンドへ `--azure-credential` を付ける。

```sh
vsc/node_modules/.bin/vsce publish --packagePath \
  "target/dist/vsc-${VERSION:?}/darwin-arm64/tsuzuri-$VERSION-darwin-arm64.vsix" \
  "target/dist/vsc-$VERSION/linux-arm64/tsuzuri-$VERSION-linux-arm64.vsix" \
  "target/dist/vsc-$VERSION/linux-x64/tsuzuri-$VERSION-linux-x64.vsix" \
  "target/dist/vsc-$VERSION/win32-arm64/tsuzuri-$VERSION-win32-arm64.vsix" \
  "target/dist/vsc-$VERSION/win32-x64/tsuzuri-$VERSION-win32-x64.vsix"
```

途中で失敗しても一括で元には戻らない。成功済み・アップロード済みで検証待ちの target を調べ、同じ CI の未公開分だけを再開する。検証待ちに対する再アップロードはしない。

## 6. Marketplace の検証完了まで確認する

**`Published` の表示や公開カタログに版が見えることだけでは完了ではない。** 5 target それぞれの `ExtensionVersionFlags.Validated`（ビットマスク `1`）が必要。

```sh
node .github/skills/vscode-publish/scripts/verify-release.mjs marketplace "$VERSION"
```

- このコマンドは認証なしの公開カタログの 5 target と各版の Validated フラグを検査し、ログインやトークンの取得は行わない。1 つでも未検証なら失敗する。
- 検証待ちなら、おおむね 1 分間隔で再確認する。非ゼロ終了の理由を読み、ネットワーク障害や validation エラーを検証待ちと混同しない。長く待つ場合は管理画面や認証済み公式 Gallery API の `validationResultMessage` も調べる。
- アップロード受理後でも、公開カタログへの反映待ちで target が不足することがある。`target set mismatch` だけを根拠に再アップロードせず、公開ログと管理側の版・target を照合する。余分な target や重複を正常な検証待ちとして扱わない。
- 検証エラー・手動審査待ちで解決できなければ「アップロード済み・公開確認未完了」と正確に報告する。公開完了を装わず、同じ版を上書きしない。
- 全 target が成功したら、版・5 target・コミット/タグ・PR・CI run・各 VSIX の SHA-256・検証時刻を公開記録へ残す。検証済み VSIX/sidecar は保持し、scratch の中間 ZIP や補助出力だけを削除する。
- 最終報告には公開版、Marketplace のリンク、5 環境すべての検証結果、version PR の状態を簡潔に示す。

## スキルの保守

チェッカーや対応表を変えたら、ネットワーク・認証・公開を使わない回帰チェックを実行する。

```sh
node --test .github/skills/vscode-publish/scripts/verify-release.test.mjs
```
