# アプリケーションの実装例

[ドキュメントのトップ](../README.md)

簡単なアプリケーションを完成コードで紹介します。個々の構文を確認する[ツアー](../tour.md)に続き、入力、データ構造、処理、表示、エラー、テストを一つの流れとして学ぶための例です。

## 作るもの

| アプリケーション | 入力と出力 | 学べること |
| --- | --- | --- |
| [料金見積もり](order-quote.md) | 単価・数量から小計・送料・合計を表示 | 入力検証、Result、checked 演算、業務ルールと表示の分離 |
| [単語カウンター](word-count.md) | 短い文章から単語別の回数を表示 | String の走査、Map、借用、所有キーの更新 |
| [TODO リスト](todo-list.md) | 追加・完了・削除のコマンド列から最終状態を表示 | 複数モジュール、union、状態の所有更新、失敗後の継続 |
| [JSON ファイル集計](file-statistics.md) | 実ファイルの測定値から統計を JSON 出力 | Node.js の I/O、WASM、バッファ ABI、数値とホストの検証 |

上から順に、一つのファイル、コレクション、複数モジュール、外部ホストへ進みます。各記事には必要なコード、プロジェクト構成、実行コマンド、期待する出力、テスト、制限を記載しています。

## 実行の前提

[入門](../get-started.md)に従ってコンパイラを用意します。コマンドはリポジトリルートを作業ディレクトリとし、アプリごとに次の独立した root を想定します。

```text
target/
    order-quote/Main.tz
    word-count/Main.tz
    todo-list/Tasks.tz
    todo-list/Main.tz
    file-statistics/Stats.tz
    file-statistics/report.mjs
    file-statistics/readings.json
```

上記は記事に掲載するコードの配置例です。このフォルダーにはドキュメントを置き、実行ソースはコードブロックとして掲載しています。独立したアプリを同じコンパイル root に混在させないでください。

最初の3例は main 内のサンプル入力を使う native コンソールアプリです。ファイル集計は実際のコマンド引数とファイルを読み、Node.js から WASM を呼びます。いずれも GUI や対話入力を提供するという意味ではありません。

## 文書内のコードを検証する

コードを試す前に、掲載されたプログラムを一時プロジェクトへ抽出して検証することもできます。

```sh
node scripts/check-docs.mjs _docs/examples/order-quote.md _docs/examples/word-count.md _docs/examples/todo-list.md _docs/examples/file-statistics.md
```

このチェックは Tsuzuri の型検査、最初の3例の native O0 / O3 の出力、4例に含まれる言語内テストを実行します。追加時に18件のテストが通っています。

Node.js のホスト例はこのスクリプトの自動検証対象ではありません。ファイル集計の記事にある build と node コマンドで確認します。記事の追加時には、WASM O0 / O3 の計算テストと、正常入力・件数制限・不正 JSON・非有限値・集計 overflow・ファイル不在のホスト動作を実行確認しています。

## サンプルの範囲

いずれも学習用の小規模アプリです。金額の丸め・税、Unicode の単語境界、永続化、リクエストサイズ制限など、本番利用で必要になる追加条件は各記事の末尾に記載します。未実装の I/O や暗黙の例外処理を言語機能として使いません。

リポジトリ直下にも[既存のサンプル](../../README.md#コンソール)があります。ここでの examples は、それらとは別に、完成した処理を説明する利用者向けドキュメントの分類です。

## 関連項目

- [言語リファレンス](../language-reference/README.md)
- [標準ライブラリ](../library-reference/README.md)
- [API 設計とスタイル](../guides/style-and-design.md)
- [実践ガイド](../guides/README.md)
