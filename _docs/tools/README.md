# 開発ツール

[ドキュメントのトップ](../README.md)

Tsuzuri の検査、生成、実行、テスト、編集、調査に使うツールです。用途ごとに入力条件と出力形式が異なります。

| 記事 | 内容 |
| --- | --- |
| [コマンドライン](command-line.md) | 全コマンド、出力形式、オプション、環境変数、入口 |
| [診断と警告](diagnostics.md) | エラーコード、複数診断、JSON、位置、deny-warnings |
| [テスト](testing.md) | test 宣言、filter、隔離、timeout、native / WASM |
| [フォーマッターと LSP](editor-tools.md) | fmt の対象範囲、全量同期、hover、definition、制限 |
| [デバッグ](debugging.md) | Debug、WASM import、trap table、DWARF、LLDB |
| [ドキュメント生成](documentation.md) | 宣言コメント、公開 API、marker と安全な置換 |
| [ビルドと cache](build-and-cache.md) | ツールチェーン、runtime 同梱、Windows、whole-build cache |

## 用途別に選ぶ

型と所有権だけを調べるなら check、コンソールで結果を確認するなら run、配布物には build を使います。WASM の実行はホスト、または test の WASM runner を使用します。

fmt は直下のソースだけを整形し、通常のプロジェクト検査は root 以下を再帰的に読み込みます。Main のないライブラリのディレクトリは doc / test が受理し、check には実際のソースファイルを指定します。

コンパイラ自身の検証は[開発用コマンド](../../README.md#検証と性能測定)、このドキュメントの検証は[文書の保守](../contributing.md)を参照してください。
