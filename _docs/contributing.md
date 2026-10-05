# このドキュメントの保守

[ドキュメントのトップ](README.md)

このフォルダーは利用者向けの日本語ドキュメントです。機能が変わったときは構文だけでなく、所有権、評価順序、境界値、失敗、対応ターゲットの説明も更新します。

## 文書の役割

| 場所 | 用途 |
| --- | --- |
| 入門・ツアー | 最初の実行と主要概念の紹介 |
| examples | 小さいアプリケーションの完成コード、実行手順、期待値、テスト |
| language-reference | 言語構文と意味 |
| library-reference | API の引数順、制約、所有権、失敗、計算量 |
| guides | ホスト連携、移行、実践、性能 |
| tools | CLI、診断、テスト、編集、生成、ビルド |
| feature-status | 全機能チケットと記事の対応、現在の制限 |
| library-reference/api | tsuzuri doc が管理するソース宣言の snapshot |

正式な契約は[言語仕様](../docs/language.md)、処理系の不変条件は[アーキテクチャ](../docs/architecture.md)と整合させます。チケットの着手前設計と現在の実装を混同せず、未実装の機能を使えるものとして書きません。

## 記事の書き方

目的、構文またはシグネチャ、実行例、意味、境界値、所有権、制限、関連項目を含めます。すべての記事に同じ見出しを強制する必要はありませんが、利用者がコードを実行できる情報をそろえます。

F# のような他言語の説明をそのまま流用せず、Tsuzuri の型、記号、評価時点、メモリモデルに合わせます。特に Array / List の記号、cold Task、Copy のコスト、UTF-16 / UTF-8 の単位を確認します。

## サンプルの検証規約

完成した Tsuzuri プログラムは言語タグ tsuzuri のコードブロックにします。既定では独立した Main.tz として型検査します。

コードフェンスの情報文字列に次の属性を付けられます。

| 属性 | 意味 |
| --- | --- |
| `run=42` | native O0 / O3 で実行し、stdout の期待値を照合 |
| `run=hello%20world` | 空白などを URL encode した期待値 |
| `project=example` | 同じ記事の同名グループを一つの複数ファイル例にする |
| `file=Traits.tt` | グループ内のファイル名。階層パスも指定可能 |

実際の例は[型クラス](language-reference/generics-and-typeclasses.md)を参照してください。Main のないライブラリ例は存在するソースファイルを入力に検査します。test 宣言を持つ例は native の test コマンドでも実行します。

構文の断片、シグネチャだけの説明、未対応構文は text のコードブロックにして、本文で断片であることを明記します。壊れた実行例を検証から逃がすためにタグを変更しないでください。

## 検証コマンド

リポジトリのルートで実行します。

```sh
cargo build --release --locked
node scripts/check-docs.mjs
```

一部の記事だけを検証する場合:

```sh
node scripts/check-docs.mjs _docs/library-reference/arrays-and-lists.md _docs/library-reference/maybe-result.md
```

TSUZURI_BIN でコンパイラのパスを変更できます。検証は OS の一時ディレクトリに例ごとの独立 root を作り、終了後に削除します。実例を同じ root に混在させません。

全体検証は Markdown の相対リンクと見出し、末尾改行、手書きの完成コード例、test 宣言、機能チケットと対応表の一致を確認します。生成 API はシグネチャなので実行例として扱いません。

このスクリプトは外部 URL の定期監視、任意の Markdown 構文の完全な解析、C / JavaScript のホスト例の自動実行までは行いません。ホストの例を変更した場合は、その build / link / instantiate / Worker の手順も実行してください。WASM、Windows、実 GPU の動作を native 検証だけで確認済みと記載しません。

## API snapshot の再生成

```sh
./target/release/tsuzuri doc std -o _docs/library-reference/api
```

このディレクトリの .tsuzuri-docs marker は生成器の管理対象を示します。中に手書きの解説を置かず、生成ページを手作業で修正しません。更新後は利用者向けの API 解説も見直します。

生成ページはソース宣言であり、組み込み操作の網羅一覧ではありません。明示制約だけの署名、内部型の不透明性などは[標準ライブラリの読み方](library-reference/README.md)の注意に従います。

## 今回の確認範囲

手書きの実行例は現行 compiler で native O0 / O3 の期待値を照合しています。C の export / extern、WASM の buffer / extern / Worker、SIMD の scalar / v128 を両レベルで実行し、WGSL 出力を確認しました。Windows 実行と実 GPU の今回の再検証は含みません。

ここでの例の成功は全機能の一般的な正しさの証明ではありません。compiler / runtime の変更には、対応する既存の Rust・E2E・参照テストを実行します。

## 参考にした情報構成

- [Microsoft Learn の F# トップ](https://learn.microsoft.com/ja-jp/dotnet/fsharp/)
- [F# 言語ガイド](https://learn.microsoft.com/ja-jp/dotnet/fsharp/language-reference/)
- [F# のツアー](https://learn.microsoft.com/ja-jp/dotnet/fsharp/tour)
- [F# の関数](https://learn.microsoft.com/ja-jp/dotnet/fsharp/language-reference/functions/)
- [F# のコンピュテーション式](https://learn.microsoft.com/ja-jp/dotnet/fsharp/language-reference/computation-expressions)

参照したのは入門、概念、構文・例・制限、API、関連項目へ進む構成です。F# の本文やサンプルを転載したものではなく、F# / .NET の機能を Tsuzuri の仕様として採用する資料でもありません。
