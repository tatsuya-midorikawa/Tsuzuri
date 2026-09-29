# Tsuzuri IDE のパッケージ作成と Marketplace 公開

この文書は、Tsuzuri の VS Code 拡張を OS／CPU 別の VSIX にまとめ、Visual Studio Marketplace に公開するための手順です。
利用者向けの操作説明は [README.md](README.md) を参照してください。

確認日: 2026-09-29。認証や Marketplace の制約は変更されるため、公開前に[公式の公開ガイド](https://code.visualstudio.com/api/working-with-extensions/publishing-extension)も確認してください。

## 1. パッケージと公開の違い

| コマンド | 処理 | Marketplace への公開 |
| --- | --- | --- |
| `npm run toolchain` | コンパイラ・LLVM・Zig SDK・対応環境の CodeLLDB を同梱 | しない |
| `npm run package` | 文書・補完データを生成し、型検査・lint・JavaScript の本番バンドルを実行 | しない。VSIX も作らない |
| `npm run vsix` | 同梱物を検証して、そのホスト用の VSIX と SHA-256 ファイルを作成 | しない |
| `npm run vsix:verify` | 既存 VSIX の同梱ツール・必須ファイル・実行権限を検証 | しない |
| `npx --no-install vsce publish --packagePath ...` | 指定した完成済み VSIX をアップロード | **する** |

**Tsuzuri のパッケージ作成には `npm run vsix` を使います。**
汎用の `vsce package` や、VSIX を指定しない `vsce publish` は使わないでください。
このリポジトリ独自の検査を通らなかったり、ネイティブバイナリ入りの成果物が platform 未指定になったりするのを避けるためです。

`vsce` は [package.json](package.json) と [package-lock.json](package-lock.json) で管理しているローカル版を使用します。グローバルインストールは不要です。
以降の `npm`／`npx` コマンドは、特記がなければ `vsc` ディレクトリで実行します。

## 2. 対象プラットフォーム

| target | ビルドホスト | ソースデバッグ |
| --- | --- | --- |
| `darwin-arm64` | macOS / ARM64 | 対応 |
| `darwin-x64` | macOS / x64 | 対応 |
| `linux-arm64` | Linux / ARM64 | 対応 |
| `linux-x64` | Linux / x64 | 対応 |
| `win32-arm64` | Windows / ARM64 | 未対応。編集・診断・ビルド・実行・テストは有効 |
| `win32-x64` | Windows / x64 | 対応 |

x86 32-bit、Web、Alpine 用のパッケージは作成しません。
Linux の拡張ホストは glibc ベースの環境を対象にします。

[scripts/package.mjs](scripts/package.mjs) は `process.platform` と `process.arch` から target を決定します。
**各 VSIX は対応する OS／CPU 上で生成し、Node.js・Rust・LLVM の CPU を一致させてください。**
ARM64 マシンで x64 の Node.js をエミュレーション実行しても、ARM64 パッケージにはなりません。
Mac で作った VSIX の名前を変更したり、`--target` だけを変更したりして Windows 版として公開しないでください。

同じリリースの全プラットフォームでは、ソースのコミット、Publisher、拡張名、拡張バージョンを揃えます。
現時点の macOS ARM64 での実機検証結果を、他のホストでの実行確認の代わりにしないでください。

## 3. ビルド環境の準備

必要なのは Node.js 24、npm、Rust stable、LLVM 21、ネットワーク接続です。
ツールチェーン生成時には公式の Zig と CodeLLDB を取得し、固定 SHA-256 と照合します。
Windows ARM64 では CodeLLDB を同梱しません。

リポジトリのルートから実行します。

```sh
cd vsc
node --version
node -p "process.platform + '-' + process.arch"
rustc -vV
npm ci
```

### macOS

Xcode Command Line Tools と Homebrew を用意し、LLVM と lld をインストールします。
ツールの再配置には `install_name_tool` と `codesign` も使用します。

```sh
brew install llvm@21 lld
export LLVM_PREFIX="$(brew --prefix llvm@21)"
export TSUZURI_WASM_LD="$(brew --prefix lld)/bin/wasm-ld"
```

### Linux

Ubuntu 等の環境に Clang 21、LLD 21、LLVM 21、`patchelf` を用意します。
現在の同梱スクリプトはライセンス収集に `dpkg-query` も使用するため、配布ビルドは CI と同じ Debian／Ubuntu 系環境を使ってください。
GUI のない環境での拡張ホスト試験には `xvfb` と VS Code が要求する共有ライブラリも必要です。
正確なインストールコマンドは [CI の Linux セットアップ](../.github/workflows/vscode.yml) にあります。

```sh
export LLVM_PREFIX=/usr/lib/llvm-21
```

### Windows

Rust stable の MSVC ツールチェーンと、そのビルドに必要な Visual Studio Build Tools／Windows SDK を用意します。
[LLVM 21.1.8 の公式リリース](https://github.com/llvm/llvm-project/releases/tag/llvmorg-21.1.8)から、CPU に対応するインストーラーを使用します。

- x64: `LLVM-21.1.8-win64.exe`
- ARM64: `LLVM-21.1.8-woa64.exe`

[CI の Windows セットアップ](../.github/workflows/vscode.yml)にはダウンロードと SHA-256 照合の例があります。
PowerShell で、実際のインストール先を指定します。

```powershell
$env:LLVM_PREFIX = 'C:\Program Files\LLVM'
```

これらは拡張機能を作る側の依存です。完成した VSIX の利用者に、別途 Rust・Clang・SDK をインストールさせる必要はありません。

## 4. バージョンと掲載情報の確認

公開前に次の内容を確定させます。Publisher が変わる場合は、次の「Publisher の登録」を先に行ってください。

- [package.json](package.json): `name`、`publisher`、`version`、`engines.vscode`、`repository`。
- [README.md](README.md): Marketplace に表示する機能、導入方法、対応環境、制限。
- [CHANGELOG.md](CHANGELOG.md): このバージョンの変更内容。
- [media/icon.png](media/icon.png): 採用済みの A 案。512 × 512 PNG。
- [../LICENSE](../LICENSE): Apache-2.0。LLVM・Zig・CodeLLDB 等の再配布条件と同梱ライセンスも確認。

現在の拡張 ID は `tmidorikawa.tsuzuri`、バージョンは `0.1.0` です。
コンパイラ自身のバージョンは [../Cargo.toml](../Cargo.toml) にあり、拡張機能のバージョンとは別に管理します。

更新公開では、新しい拡張バージョンを先に設定し、すべての target を作り直します。例えば次のコマンドは `0.1.1` に更新します。

```sh
npm version 0.1.1 --no-git-tag-version
```

このコマンドはパッケージのバージョンと lockfile を更新しますが、コミットやタグは作りません。
以降のファイル名の例は `0.1.0` なので、公開する版に読み替えてください。
`vsce publish patch` のように公開時に自動で版を上げる方法は使いません。事前検証した VSIX と公開される版がずれるのを防ぎます。

## 5. VSIX の作成と検証

環境変数を設定した同じターミナルで実行します。

```sh
npm run toolchain
npm run test:unit
npm run test:toolchain
npm test
npm run vsix
npm run test:installed
```

Linux の GUI なし環境では、上記の `npm test` と `npm run test:installed` を次に置き換えます。

```sh
xvfb-run -a npm test
xvfb-run -a npm run test:installed
```

処理の内容は次のとおりです。

1. `toolchain` が現在のソースからコンパイラをビルドし、対応する LLVM・SDK・デバッガーとライセンスを収集します。
2. `test:unit` が共通処理、プラットフォーム分岐、TextMate 文法を検証します。
3. `test:toolchain` が同梱物を別パスへ移し、システムの Clang／SDK に頼らず native／WASM の O0／O3 を試験します。
4. `test` が実際の VS Code 上で編集・診断・ビルド・実行・テストと、対応環境のデバッグを試験します。
5. `vsix` が型検査、lint、バンドル、秘密情報検査を行い、生成したアーカイブの必須ファイル・同梱ツールのハッシュ・実行権限を検証します。
6. `test:installed` が空のプロファイルへ完成した VSIX をインストールし、開発フォルダーではなく配布物で IDE 試験を行います。

Windows ARM64 の IDE 試験は、デバッグが利用不能であることと、それ以外の機能が動くことを確認します。
生成に成功しただけでは公開せず、対象ホストの試験を通ったものを使ってください。

macOS ARM64 の成果物の例です。

```text
dist/tsuzuri-0.1.0-darwin-arm64.vsix
dist/tsuzuri-0.1.0-darwin-arm64.vsix.sha256
```

既存 VSIX を再検証する場合は、同じホストと対応する `toolchain` を用意して `npm run vsix:verify` を実行します。
**このコマンドは VSIX 全体の SHA-256 ファイルを再生成します。ダウンロードした配布物の真正性確認の代わりには使わないでください。**
移送した成果物は、ビルド時の信頼できる `.sha256` と照合します。チェックサム中のファイル名は配布ディレクトリ相対です。

```sh
cd dist
shasum -a 256 -c tsuzuri-0.1.0-darwin-arm64.vsix.sha256
cd ..
```

Linux では `sha256sum -c`、Windows では `Get-FileHash -Algorithm SHA256` を使って同じ値を確認できます。
`.vsix` と `.sha256` を出所不明の同じ場所から取得しても、それだけで真正性を保証できるわけではありません。

手元の VS Code に導入する場合は **Extensions > Install from VSIX...** を使うか、次を実行します。

```sh
code --install-extension ./dist/tsuzuri-0.1.0-darwin-arm64.vsix
```

## 6. CI で全プラットフォームを作成する

[Tsuzuri IDE ワークフロー](../.github/workflows/vscode.yml)は、6 種類のホストでパッケージ作成と試験を行います。

1. 同じ公開用コミットに拡張バージョンと掲載情報を揃えます。
2. GitHub の **Actions > Tsuzuri IDE > Run workflow** で、そのコミットを含むブランチを選択して実行します。
3. 6 target のジョブが成功し、特に `Installed VSIX integration` が通ったことを確認します。
4. 各ジョブの `tsuzuri-<target>` artifact をダウンロードし、VSIX とチェックサムを保存します。

ワークフローは PR と `vsc-v*` タグの push でも起動します。
**現在の CI は成果物を保存するだけで、Marketplace には公開しません。** 公開用の認証情報も要求しません。
未検証の target を公開済みとして案内しないでください。一部の target だけ公開する場合は対応状況を掲載情報に明記します。

## 7. Publisher の登録

1. [Marketplace の管理画面](https://marketplace.visualstudio.com/manage)に Microsoft アカウントでサインインします。
2. **Create publisher** から Publisher ID と表示名を登録します。既存 Publisher を使う場合は公開権限を確認します。
3. [package.json](package.json) の `publisher` と登録した ID を一致させます。

設定値 `tmidorikawa` の取得・所有状況は、このリポジトリでは保証していません。
Publisher ID は作成後に変更できません。別の ID を使う場合は、**初回公開前に**拡張 ID 全体を確定させてください。

Publisher を変更する場合、少なくとも以下も更新してから VSIX を作り直します。

- [package.json](package.json) の `contributes.configurationDefaults.[tsuzuri].editor.defaultFormatter`。
- [src/test/extension.test.ts](src/test/extension.test.ts) の `getExtension` に渡す拡張 ID。
- 公開 URL、インストールコマンド、ドキュメント等に記載した `tmidorikawa.tsuzuri`。

既に公開した拡張では、`publisher`／`name` の変更は単なるバージョン更新ではありません。既存利用者の自動更新や設定への影響を確認してください。

## 8. CLI の公開認証

### 認証方式の選択

公式ガイドでは、自動公開に Microsoft Entra ID と Workload Identity Federation を使う方式を推奨しています。
**グローバル PAT は 2026-12-01 に廃止予定と案内されています。** 以下の PAT 手順は移行期間の手動公開向けです。
廃止後の運用や新しい自動公開パイプラインは、[公式の Entra ID 認証手順](https://code.visualstudio.com/api/working-with-extensions/publishing-extension#secure-automated-publishing-to-visual-studio-marketplace)に従って構成してください。

Entra ID 方式では、公開に用いる ID を Marketplace Publisher のメンバーに追加し、必要な公開権限を付与して、実行環境で認証を成立させます。
その設定済み環境で `vsce verify-pat --azure-credential`／`vsce publish --azure-credential` を使用できます。
**`--azure-credential` を付けるだけで ID の作成や権限付与が行われるわけではありません。** 現在の GitHub Actions にこの認証設定は含まれていません。

### 移行期間の PAT による手動認証

1. Azure DevOps の組織を用意し、公開権限を持つ Microsoft アカウントでサインインします。
2. **User settings > Personal access tokens > New Token** を開きます。
3. 現行の公式手順に従い、Organization は **All accessible organizations**、Scopes は **Marketplace > Manage** を選びます。表示されない場合は **Show all scopes** を開きます。
4. 必要最小限の有効期間で作成し、安全な資格情報ストアに保管します。
5. 次を実行し、PAT はターミナルの非表示入力へ直接入力します。

```sh
npx --no-install vsce login tmidorikawa
npx --no-install vsce verify-pat tmidorikawa
```

`tmidorikawa` は実際の Publisher ID に読み替えます。
PAT をコマンド引数、ソース、`.env`、Markdown、チャット、スクリーンショット、ビルドログに書かないでください。
共有環境に認証情報を残す必要がなくなった場合は `npx --no-install vsce logout tmidorikawa` を使います。漏えいの疑いがある PAT はログアウトだけでなく発行元で失効させます。

## 9. Marketplace へ公開する

### 完成済み VSIX を CLI でアップロード

ここからのコマンドは実際に Marketplace の状態を変更します。Publisher、バージョン、検証結果、チェックサムを確認してから実行してください。

PAT でログイン済みの場合の例です。

```sh
npx --no-install vsce publish --packagePath ./dist/tsuzuri-0.1.0-darwin-arm64.vsix
```

Entra ID 認証を構成済みの環境では、次を使います。

```sh
npx --no-install vsce publish --azure-credential --packagePath ./dist/tsuzuri-0.1.0-darwin-arm64.vsix
```

`--packagePath` では既存の VSIX を公開するため、このマシンで再ビルドする必要はありません。
target はアーカイブのメタデータに含まれます。**公開時に `--target` を付け直さないでください。**
検証が済んだプラットフォーム別 VSIX を変更せず集め、1台の公開用マシンからアップロードできます。

全6種類を `vsc/dist` に集めた場合の例です。すべて同じバージョンのファイルを明示します。

```sh
npx --no-install vsce publish --packagePath ./dist/tsuzuri-0.1.0-darwin-arm64.vsix ./dist/tsuzuri-0.1.0-darwin-x64.vsix ./dist/tsuzuri-0.1.0-linux-arm64.vsix ./dist/tsuzuri-0.1.0-linux-x64.vsix ./dist/tsuzuri-0.1.0-win32-arm64.vsix ./dist/tsuzuri-0.1.0-win32-x64.vsix
```

Entra ID 方式なら同じコマンドに `--azure-credential` を追加します。
複数ファイルの公開は、全件が一括で成功・失敗する操作ではありません。
途中で失敗した場合は管理画面で成功した target を確認し、不足した VSIX だけを再実行します。
同じバージョン・同じ target の差し替えは行わず、内容の修正には新しいバージョンを使ってください。
未対応環境への誤配布を避けるため、target のない universal パッケージは公開しません。

### 管理画面からアップロード

CLI 認証を使わず、[Marketplace の管理画面](https://marketplace.visualstudio.com/manage)から完成済み VSIX をアップロードすることもできます。

1. 対象 Publisher を選択します。
2. 初回は **New extension > Visual Studio Code** を選択し、検証済み VSIX をアップロードします。
3. 既存拡張では対象拡張の更新操作から、新しい VSIX をアップロードします。
4. 同じリリースの各 target について実施し、版とプラットフォームの一覧を確認します。

ボタン名は画面の更新で変わる場合があります。アップロード後の検証・マルウェア検査・掲載処理が完了したことまで確認してください。

## 10. 公開後の確認と更新

- 管理画面で Publisher、バージョン、target の一覧、検証結果を確認する。
- Marketplace の説明、PNG アイコン、ライセンス、リンクが正しく表示されることを確認する。
- 対象プラットフォームのクリーンな VS Code からインストールして動作を確認する。
- Windows ARM64 はデバッグのみ非対応と表示され、他の機能が使用できることを確認する。
- 公開したコミット、全 VSIX、ビルド時の SHA-256、CI run、実機試験結果をリリース記録として残す。

Publisher と拡張名が現在の設定なら、公開先 URL は `https://marketplace.visualstudio.com/items?itemName=tmidorikawa.tsuzuri` です。
この URL の記載は、公開済みであることを意味しません。

```sh
code --install-extension tmidorikawa.tsuzuri
```

以後の更新も、バージョン更新、対象ホストでの再生成・試験、完成済み VSIX のアップロードの順で行います。
不具合の修正を既存バージョンへ上書きする運用は避けてください。
この文書は通常リリース用です。pre-release を導入する場合は[公式の pre-release 方針](https://code.visualstudio.com/api/working-with-extensions/publishing-extension#pre-release-extensions)を確認し、版の採番と検証手順を別途決めます。

## 11. よくある問題

| 症状 | 確認すること |
| --- | --- |
| `npm run package` の後に VSIX がない | 配布物の生成は `npm run vsix` |
| `LLVM_PREFIX`／Clang のエラー | LLVM 21 の絶対パス、CPU、Clang の存在。Homebrew は lld の別パスも確認 |
| `Toolchain host mismatch` | Node.js・Rust・LLVM・同梱物の CPU／OS を一致させ、対象ホストで再生成 |
| ZIP 作成前に非常に長時間かかる | 独自の `npm run vsix` を使用。通常の vsce に巨大な LLVM バイナリの文字列走査をさせない |
| `Secret scan failed` | 原因を調査して秘密情報を除去する。検査を無効化して公開しない |
| `401`／`403` | Publisher 権限、認証 ID、PAT の期限・組織・Marketplace Manage scope、認証方式の廃止状況 |
| Publisher／拡張名が既に存在する | 管理権限と ID を確認。初回公開前なら実際に利用できる ID に揃えて作り直す |
| 同じバージョン・target が既に存在する | 不足 target の追加か、内容変更による新バージョンかを区別する |
| アイコン／README 画像で公開エラー | アイコンは PNG。README／CHANGELOG の画像 URL は HTTPS で、独自 SVG を使わない |
| 別 OS でコンパイラが起動しない | ファイル名ではなく VSIX 内の target とバイナリを確認。対象 OS／CPU で生成・試験する |
| ファイル数の警告 | SDK 同梱により多くなる。許可リストと内容を確認し、必要な SDK ファイルを無根拠に削らない |

公開停止や削除が必要な場合は、管理画面の **Unpublish** と **Remove** を混同しないでください。
`vsce unpublish` は拡張の削除操作で、復元できない影響があります。更新のやり直しや公開停止のために安易に実行しないでください。

## 公式資料

- [Publishing Extensions](https://code.visualstudio.com/api/working-with-extensions/publishing-extension)
- [Platform-specific extensions](https://code.visualstudio.com/api/working-with-extensions/publishing-extension#platform-specific-extensions)
- [Extension Manifest](https://code.visualstudio.com/api/references/extension-manifest)
- [Publisher 管理画面](https://marketplace.visualstudio.com/manage)
- [vsce](https://github.com/microsoft/vscode-vsce)
- [グローバル PAT の廃止案内](https://devblogs.microsoft.com/devops/retirement-of-global-personal-access-tokens-in-azure-devops/)
