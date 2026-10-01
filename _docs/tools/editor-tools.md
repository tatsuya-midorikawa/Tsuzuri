# フォーマッターと LSP

[ドキュメントのトップ](../README.md)

CLI の整形と、エディター向けの言語サーバーを提供します。どちらもソースの意味を変えないことを優先します。rename と quick fix は、編集後のプロジェクトを内部で再解析して意味が変わらないことを確かめてから返します。

## フォーマッター

```sh
./target/release/tsuzuri fmt examples/hello/Main.tz
./target/release/tsuzuri fmt --check examples/hello
```

入力は `.tz` / `.tt` / `.tc` のファイル、またはディレクトリです。**fmt のディレクトリ入力は直下だけ**が対象です。コンパイラの再帰的なソース探索と同じ範囲ではありません。

LLVM、型検査、実行入口は必要ありません。--check はファイルを変えず、整形差分があれば終了コード 1 です。通常の fmt は変更するファイルだけを書き換えます。

## 整える内容

4 スペースのインデント、演算子・区切りの空白、コメント外の末尾空白、末尾一改行を整えます。行幅に応じた折り返し、宣言の並べ替え、式の組み替えは行いません。

呼び出しと索引、空白適用と二項演算の違いを維持します。コメント本文、doc comment、リテラルは保持し、BOM も保ちます。生成する改行は既存の CRLF と lone LF の多い方で、同数なら LF です。

整形前後の AST を位置情報以外で比較し、意味が変わる場合は書き込みません。単なるテキスト置換による整形ではありません。

## 書き込みの安全性

既存の権限を保持して atomic replace します。symlink と特殊ファイルは拒否します。hard link は rename により切り離し、ほかのリンク先を変更しません。

この文書内のサンプルを整形する際も、Markdown 本体に fmt を実行するものではありません。fmt は Tsuzuri ソース向けです。

## LSP の起動

```sh
./target/release/tsuzuri lsp
```

LSP クライアントから標準入出力のサーバーとして起動します。対話 REPL ではありません。パス、--json、ビルドオプションは付けません。stdout は JSON-RPC framing 用なので、ログを混ぜないようにします。

VS Code の通常設定だけで任意の言語サーバーが自動起動するわけではありません。利用する LSP クライアント側で、実行ファイル、引数 lsp、stdio transport、対象拡張子、workspace root を設定します。存在しない Tsuzuri 専用設定キーや拡張機能を前提にしません。

## 現在の capability

| 機能 | 対応 |
| --- | --- |
| didOpen / didChange / didClose | 未保存バッファの全量同期 |
| publishDiagnostics | プロジェクト内の複数診断 |
| hover | 型と宣言に添えた説明 |
| definition | 関数、レコード、ローカル等の定義位置 |
| documentSymbol | ファイル内の宣言一覧 |
| references / documentHighlight | 名前の定義と参照。`def` と `fn` の両方の宣言名を含む |
| prepareRename / rename | ローカル、関数、const、record、union、union case、フィールドの名前変更 |
| workspace/symbol | 解析済みプロジェクトの宣言を名前の部分一致で検索 |
| completion | `.` の後のフィールド・モジュールのメンバー・union case、見えているローカル、同じモジュールの宣言、モジュール名、予約語 |
| signatureHelp | 呼び出し中の関数の引数と、入力中の引数の位置（カリー化した適用は空白区切りで数える） |
| semanticTokens/full | 名前の出現だけを色分けする。予約語・リテラル・コメントは TextMate 文法に任せる |
| codeAction | `W1001`（未使用のローカル）に `_` を前置する quick fix |
| formatting | CLI の `tsuzuri fmt` と同じ結果で文書全体を置き換える。構文エラーのときは何も返さない |
| cancelRequest、shutdown / exit | リクエスト中止と終了 |

変更は短い間隔で集約して解析します。未保存の新規ソースも overlay として扱い、ディスクを書き換えません。解析失敗後に古い成功時の型情報を hover・definition・references・rename で返すことはしません。入力中で解析に失敗している間は、completion・signatureHelp・semanticTokens だけが直前に成功した解析を、変更されていない前後のテキストに写して使います。

### rename と quick fix の制限

次の場合は rename を拒否し、理由をエラーで返します。

- プロジェクトにエラーがある。
- 標準ライブラリの定義、`export` した関数（export 名は ABI の一部）。
- 型別名、class、method、extern、test、モジュール（未対応）。
- 新しい名前が識別子でない、予約語である、同じ名前空間の定義や見えているローカルと衝突する。
- 10,000 か所を超える変更、または再解析で診断が増える・名前の結び付きが変わる変更。

quick fix はサーバーが公開した診断だけから作り、同じ検査を通ったものだけを返します。

## root と位置の単位

client が通知した workspace root のうち、対象ファイルを含む最も深い root を使います。該当がなければファイルの親を使います。階層モジュールを扱うときは、プロジェクト root を正しく通知します。

client が UTF-8 position を提案すれば UTF-8 を使い、そうでなければ LSP 既定の UTF-16 code unit を使います。位置の行・character は LSP の 0 始まりです。CLI の 1 始まりの Unicode 文字列 column をそのまま送らないでください。

inlay hint、call hierarchy、関数の抽出などの自動リファクタリング、project root をまたぐ rename は提供しません。サーバーが公開する capability に従ってクライアント機能を有効にします。

## 関連項目

- [CLI](command-line.md)
- [診断の位置情報](diagnostics.md)
- [モジュールと root](../language-reference/modules-and-packages.md)
