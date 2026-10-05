# Tsuzuri IDE の開発

この文書は、Tsuzuri の VS Code 拡張機能を開発・検証する人向けです。
利用者向けの使い方は [README.md](README.md)、パッケージ作成と Marketplace への公開手順は [Publishing.md](Publishing.md) を参照してください。

Tsuzuri の編集・ビルド・実行・テスト・ソースデバッグを VS Code に統合します。
OS／CPU 別 VSIX にコンパイラとビルド用ツールチェーンを同梱し、利用者による Rust・Clang・Zig・OS SDK の追加インストールは不要です。

## 導入

1. VS Code 1.103 以降を用意します。
2. 拡張ホストの OS／CPU に合った `tsuzuri-0.1.0-<target>.vsix` を、拡張機能ビューの **Install from VSIX...** からインストールします。
3. Tsuzuri のプロジェクトフォルダーを開いて信頼し、`.tz`・`.tt`・`.tc` を開きます。

新規プロジェクトは **Tsuzuri: New Project** で空フォルダーを選び、名前空間を入力します。コンパイラの `tsuzuri new <folder> --namespace <name>` が、`namespace` を書いた Tsuzuri.toml、同じ名前空間を宣言する Main.tz、.gitignore を作成します。既存ファイルは上書きしません。
通常のプロジェクトに tasks.json／launch.json の作成は不要です。実行ファイルは `.tsuzuri/` 以下へ生成します。
デバッグ対応環境では、初回 F5 時に同梱の公式 CodeLLDB VSIX を自動インストールします。Marketplace への接続は不要です。
既に CodeLLDB が入っている場合はそれを使い、勝手にダウングレードしません。

## 配布対象

| target | 編集・診断・整形・ビルド・実行・テスト | ソースデバッグ |
| --- | --- | --- |
| darwin-x64 | 有効 | CodeLLDB |
| darwin-arm64 | 有効 | CodeLLDB |
| linux-x64 | 有効 | CodeLLDB |
| linux-arm64 | 有効 | CodeLLDB |
| win32-x64 | 有効 | CodeLLDB |
| win32-arm64 | 有効 | 未対応。ほかの機能には影響しません |

x86 32-bit は非対応です。Windows ARM64 ではデバッグ用依存を要求せず、デバッグボタンも表示しません。
Linux は glibc ベースの VS Code 対応環境を対象とし、Alpine/musl の拡張ホストは対象外です。生成する Linux プログラムは同梱 musl を使います。
Remote SSH／WSL／Dev Containers では拡張機能をリモート側へ導入し、その拡張ホストに合う VSIX を選択します。Web 版 VS Code／仮想ワークスペースは対象外です。

**検証状況:** macOS ARM64 では実際の VS Code 上の編集からデバッグまで確認済みです。
他の OS／CPU の実機試験は、この作業環境では実行していません。6 環境の CI は、そのホスト上のテストに成功した場合だけ VSIX を保存します。
OS の最小版は VS Code、同梱 LLVM バイナリ、CodeLLDB の要件をすべて満たす必要があります。別 OS での動作保証をクロスコンパイルだけで主張しません。

## 編集機能

- シンタックスハイライト、入れ子コメント、型変数、文字・文字列、数値接尾辞。
- 未保存バッファのエラー・警告、UTF-16 位置、外部ファイル変更の再診断。
- 型とドキュメントのホバー、定義ジャンプ、アウトライン、パンくず。
- キーワード・型・標準ライブラリ関数の基本補完、関数・ラムダ・test 等のスニペット。
- 公式 formatter による整形、括弧補完、コメント切り替え、インデント、折りたたみ。
- 複数ワークスペース、プロジェクト別言語サーバー、再起動コマンド。
- **Tsuzuri: Open Documentation** から利用できる同梱日本語ハンドブック。

型に基づく完全な補完、参照検索、rename、signature help、code action はまだ提供しません。
整形は未保存テキストを専用一時領域へ渡し、公式 formatter の AST 保存検査に成功した編集だけを返します。ディスク上のソースを直接上書きしません。

## ビルド・実行・テスト

エディター右上の実行・デバッグ・ビルドボタン、コマンドパレット、ステータスバーの **Tsuzuri** から操作します。
**Tasks: Run Build Task** にも Tsuzuri のタスクを提供します。ビルド・実行・テストの前に保存します。
Run は統合ターミナルを使うため、IO.read_line の標準入力も利用できます。

Test Explorer はコンパイラの `test --list --json` で検出し、検出時にテスト本体や LLVM を実行しません。
同名テストも index で区別し、個別実行、全件実行、失敗位置、所要時間、キャンセル、O0／O3 のプロファイル、保存後の再検出を扱います。
宣言の編集で index が変わった古いテスト選択は拒否し、誤ったテストを実行しません。

**Tsuzuri: Build WebAssembly** は同梱 Clang／wasm-ld で WASM を生成します。
WASM のホスト実行・ブラウザーデバッグ、CLI の `test --target wasm32` に必要な外部 Node.js は、この拡張機能の通常の native テスト／デバッグとは別です。

## デバッグ

F5 または **Tsuzuri: Debug Project** は `-O0 -g --trap-info` でビルドして CodeLLDB を起動します。
ブレークポイント、ステップ、呼び出しスタック、ローカル変数、Watch、Debug Console、統合ターミナルが利用できます。
macOS の `.dwarf` は自動で読み込みます。現状は DWARF に記録された低水準の型表示で、Tsuzuri 専用 pretty printer や完全な Tsuzuri 式評価はありません。
テスト単体のソースデバッグは未対応です。Windows ARM64 はデバッグ開始時に非対応を説明し、通常の実行・テストは利用できます。
必要な場合だけ launch.json で `type: "tsuzuri"`、`request: "launch"`、`project`、`args`、`env`、`stopOnEntry` を指定します。

## 設定

| 設定 | 既定値 | 用途 |
| --- | --- | --- |
| tsuzuri.projectPath | 空 | workspace からの相対、または絶対プロジェクトディレクトリ。空なら Main.tz／Tsuzuri.toml を探索 |
| tsuzuri.optimization | 3 | build／run の最適化。debug は常に O0 |
| tsuzuri.denyWarnings | false | 警告でも check／build／run を失敗させる |
| tsuzuri.compilerPath | 空 | 開発用コンパイラの絶対パス。通常は変更不要 |
| tsuzuri.toolchainPath | 空 | 開発用ツールチェーンの絶対パス。通常は同梱物を使用 |

通常は最も近い Main.tz／Tsuzuri.toml をプロジェクト root とします。親 root の下に独立プロジェクトを置く構成は、別 workspace folder に分けるか projectPath を明示してください。
信頼していない workspace ではコード・コンパイラ・設定由来のプログラムを起動しません。テレメトリは追加していません。
プロセスは引数配列で起動し、ユーザーのパスをシェルのコードとして評価しません。

## 拡張機能の開発

以下は拡張機能を作る人向けです。完成した VSIX の利用者には不要です。
Rust stable、Node.js 24、npm、LLVM 21 が必要です。macOS の再配置には install_name_tool／codesign、Linux には patchelf、Windows には Rust のビルド用 MSVC 環境を使います。

```sh
cd vsc
npm ci
LLVM_PREFIX=/absolute/path/to/llvm21 npm run toolchain
npm run test:unit
npm run test:toolchain
npm test
npm run vsix
npm run test:installed
```

Homebrew の LLVM と lld が別パッケージの場合は `TSUZURI_WASM_LD=/absolute/path/to/wasm-ld` も指定します。
Windows は環境変数 LLVM_PREFIX に LLVM 21 のインストール先を指定します。ターゲットの CPU と Node／Rust／LLVM の CPU を一致させてください。
`npm test` は隔離された VS Code 1.103.2 を取得します。既存 VS Code を使う場合は VSCODE_EXECUTABLE_PATH を実行ファイルの絶対パスにします。
Linux の GUI なし環境では `xvfb-run -a npm test` を使います。

`npm run toolchain` は LLVM、再配置した共有ライブラリ、Zig 0.16.0 の SDK／libc、コンパイラ、対応環境の CodeLLDB 1.12.3 を同梱します。
LLVM IR のオブジェクト化は Clang、C runtime とリンクは SDK を内包する Zig が担当します。意味を変更する fast-math は追加しません。
Windows ARM64 では Zig 0.16.0 がリンク中に異常終了するため、Zig の代わりに llvm-mingw 20251216（LLVM 21.1.8）の MinGW-w64 sysroot と compiler-rt を同梱し、C runtime とリンクも同梱 Clang と ld.lld が担当します。
ダウンロードは固定 SHA-256 で検証し、ライセンスを同梱します。生成物は git 管理せず、npm の lockfile とビルドスクリプトを管理します。

`npm run vsix` は manifest と全ツールのハッシュを検証してから platform-specific VSIX を作り、アーカイブ内の全ツールのハッシュ・実行権限・必須ファイルを再検証します。
`dist/tsuzuri-0.1.0-<target>.vsix` と SHA-256 ファイルが成果物です。`npm run vsix:verify` で再検証できます。
検証は native／WASM O0／O3、同梱物の別パス移動、システム Clang／SDK の遮断、IO、並列テスト、Unicode 診断、複数 root、実際の LLDB 停止と変数を含みます。
`test:installed` は空の拡張プロファイルへ VSIX をインストールし、開発フォルダーではなく配布物のコードで同じ IDE 試験を行います。macOS ARM64 では VS Code 1.103.2 と作業環境の VS Code で確認済みです。
秘密情報検査は拡張コードと同梱文書に対して実行し、大きなバイナリ群は manifest の全件ハッシュ検証で管理します。
CI は自動公開せず成果物を保存します。Marketplace の publisher 登録・署名・公開は別のリリース作業です。

## Marketplace の掲載文

[README.md](README.md) は Marketplace と拡張機能の詳細画面に表示する利用者向けの説明です。ビルド手順や検証状況はこの文書に書きます。
README のスクリーンショットは [images](images) に置きます。VSIX には含めず、`npm run vsix` が README 内の相対リンクを GitHub の `vsc/` 以下の URL に書き換えます。
Marketplace は公開後も GitHub の既定ブランチから画像を読み込むため、画像を push してから公開してください。
`npm run resources` は同梱ハンドブック用に README と画像をコピーするため、オフラインでも同じ画面を表示できます。
