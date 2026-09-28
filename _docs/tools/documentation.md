# ドキュメントコメントと API 文書生成

[ドキュメントのトップ](../README.md)

`///` は宣言へ説明を添える構文です。LSP hover と doc コマンドが利用し、通常の型検査や生成 IR の意味は変えません。

## コメントを書く

ライブラリの `Numbers.tz` の例です。

```tsuzuri project=api-docs file=Numbers.tz
/// 入力に一つ加えます。整数の演算は型幅で折り返します。
def increment :: i64 -> i64 = \value -> value + 1

/// 計算で使用する固定値です。
const Answer: i64 = 42
```

説明は fn ではなく def の直前に置きます。複数の `///` 行は改行で連結し、直後の空白を一つだけ取り除きます。`////` の本文は `/` から始まります。Markdown とコードフェンスを説明文として使えます。

## 対象の宣言

def、export / private / extern を含む def、record、union、type、const、class、class 内の method def に添えられます。

fn / let の実装、instance、フィールド、ローカル束縛、計算式の文、test、entry code、ファイル末尾への配置は E0002 です。通常の // とブロックコメントは API 説明にはなりません。

## 生成する

上の例を `target/doc-demo/Numbers.tz` に置いた場合:

```sh
./target/release/tsuzuri doc target/doc-demo -o target/doc-demo-api
```

Main は不要です。全ソースを検査してから公開宣言だけを Markdown へ出力します。LLVM / Clang は不要です。出力にはモジュールごとのページと index.md があり、階層モジュールは Geometry.Point.md のような名前になります。

ページ順は決定的に並べ、ページ内の署名はソース順です。型変数、制約、region、アクティブパターンの宣言名を保持します。const は型だけを表示し、初期化式は表示しません。instance の実装は文書化せず、class は method を含みます。

## 標準ライブラリの生成

```sh
./target/release/tsuzuri doc std -o target/std-api
```

完全な標準ライブラリのファイル構成を持つ std ディレクトリを入力にできます。同一 std モジュールの複数ソースは一つのページへまとめます。

生成対象はソースの宣言です。コンパイラ内の組み込み関数や、推論されるすべての制約を独立した一覧として生成する機能ではありません。Vec、Int、Simd、Math などの全体像には利用者向けの API 解説を併用します。

Map、Set、Seq、Gpu の不透明性などは型検査の契約です。ソース宣言の内部フィールドが表示されても、利用者による構築・分解が可能とは限りません。

## 出力ディレクトリの保護

新規なら親ディレクトリも作成します。既存の出力先には、正しい識別文を持つ通常ファイル `.tsuzuri-docs` が必要です。marker は 4,096 byte 以内で、出力や marker の symlink は拒否します。

ソースを含む出力先、index.md と衝突するモジュール、大文字小文字だけが異なるページ名の衝突も E2003 です。

全ページを同じファイルシステムの一時ディレクトリで完成させ、旧出力を退避してから置換します。公開に失敗したら復元を試み、復元不能なら退避場所を診断して残します。

正しい marker のあるディレクトリは**全体が生成器の管理下**です。手書き文書を混在させないでください。このリポジトリでは手書きの `_docs` 全体を出力先にせず、生成専用の下位ディレクトリを使います。

## オプションと安全性

--json は診断だけを JSON lines にします。文書の JSON 出力を生成するオプションではありません。target / emit / optimization / cpu / debug などのビルド専用オプションは E2000 です。

説明の Markdown、HTML、コードフェンスをそのまま出力します。信頼できない作者の文書を表示する renderer は HTML や危険なリンクを適切に制限してください。

doc コマンドは doc-test を実行しません。本ドキュメントの検証スクリプトは、言語の doc 機能とは別のリポジトリ用チェックです。

## 関連項目

- [LSP hover](editor-tools.md)
- [生成済み std 宣言](../library-reference/api/index.md)
- [可視性](../language-reference/modules-and-packages.md)
