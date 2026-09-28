# Tsuzuri ドキュメント

Tsuzuri は、関数型の式、静的な型検査、所有権と借用を組み合わせたプログラミング言語です。Rust 製のコンパイラから LLVM IR を生成し、ネイティブ実行ファイルと WebAssembly にコンパイルします。計算を Tsuzuri に、画面・ファイル・ネットワークなどの外部との接続をホストに分けて構成します。

このドキュメントは **Tsuzuri 0.1.0 の現在の実装**を対象とする日本語の利用者向けガイドです。構文だけでなく、評価順序、値の所有権、失敗時の動作、対応ターゲットを説明します。設計目標は実装済み機能と区別します。

## 作業の開始

- [入門](get-started.md): 開発環境を用意し、最初のプログラムとテストを実行する。
- [Tsuzuri のツアー](tour.md): 短い実行例で値、関数、データ、借用、失敗、タスクを学ぶ。
- [アプリケーションの実装例](examples/README.md): 料金見積もり、単語カウント、TODO 管理、JSON ファイル集計を完成コードで学ぶ。
- [F# からの移行](guides/from-fsharp.md): 似た記法の異なる意味を確認する。

## 言語を調べる

[言語リファレンス](language-reference/README.md)では、構文・意味・制約を機能ごとに説明します。

| 分野 | 記事 |
| --- | --- |
| 基礎 | [ソースと予約語](language-reference/lexical-and-layout.md)、[値と定数](language-reference/values-and-constants.md)、[関数](language-reference/functions.md)、[演算子](language-reference/expressions-and-operators.md) |
| 型とデータ | [型推論](language-reference/types.md)、[レコードと別名](language-reference/records.md)、[union と再帰型](language-reference/unions.md) |
| 多相性 | [型クラス](language-reference/generics-and-typeclasses.md)、[高階型](language-reference/higher-kinds.md)、[自動導出](language-reference/deriving.md) |
| メモリ | [所有権と new](language-reference/ownership.md)、[lifetime と借用フィールド](language-reference/lifetimes.md) |
| 制御 | [条件と反復](language-reference/control-flow.md)、[パターンと網羅性](language-reference/patterns.md)、[アクティブパターン](language-reference/active-patterns.md) |
| 計算の合成 | [コンピュテーション式](language-reference/computation-expressions.md)、[タスク](language-reference/tasks.md) |
| 表現と構成 | [数値](language-reference/numbers.md)、[文字列と文字](language-reference/strings-and-characters.md)、[モジュールとパッケージ](language-reference/modules-and-packages.md) |

## 標準ライブラリ

[標準ライブラリと組み込み API](library-reference/README.md)に引数順、型制約、所有権、境界条件をまとめています。

- [Option / Result](library-reference/option-result.md)、[Array / List / スライス](library-reference/arrays-and-lists.md)、[Vec](library-reference/vec.md)、[Map / Set](library-reference/map-set.md)、[Seq](library-reference/sequences.md)
- [文字列 API](library-reference/text.md)、[Math と順序付き集計](library-reference/math.md)、[Int](library-reference/integers.md)、[表示と解析](library-reference/formatting-and-parsing.md)
- [IO と標準入出力](library-reference/io.md): IO アクション、対話入力、EOF と失敗、native / WASM の接続。
- [Parallel](library-reference/parallel.md)、[Simd](library-reference/simd.md)、[基本組み込み・Debug・Test](library-reference/builtins.md)
- [std のソース宣言一覧](library-reference/api/index.md): 自動生成の補助資料。組み込み API と不透明型の制約は解説を併用します。

## 実践とツール

- [実践ガイド](guides/README.md): C、WASM、Worker、GPU、性能、API 設計。
- [開発ツール](tools/README.md): CLI、診断、テスト、fmt / LSP、デバッグ、文書生成、cache。
- [全機能の対応状況](feature-status.md): 全55機能チケットと詳細記事の対応、部分対応と未実装。

GPU は実験的な CPU 参照・WGSL・WebGPU host の段階です。Windows の実装はありますが、Windows 上の実行検証は未完了です。将来の計画を現在の提供機能として扱わないでください。

## この言語の前提

| 項目 | Tsuzuri の考え方 |
| --- | --- |
| 値と状態 | `let` は不変。`let mut` と排他的な借用でローカル状態を扱う |
| 関数 | `def name :: 型 = ラムダ式` で型と実装をまとめる |
| メモリ | GC ではなく所有権の移動、借用、スコープ終了時の解放を使う |
| 多相性 | 型パラメーターと型クラスをコンパイル時に具体化する |
| エラー | 回復可能な失敗は `Option` / `Result`、契約違反はトラップとして扱う |
| 外部連携 | `export def` と `extern def` により C / WASM ホストへ明示的に接続する |
| 性能 | 数値と所有権の意味を保つ。SIMD・並列化・GPU という名称だけで高速性を保証しない |

## 文書の読み方

例に明記がなければ、単一ファイルのプログラムは独立したディレクトリの `Main.tz` として扱います。複数ファイルの例は、示したファイル名とディレクトリ構成を保ちます。コンパイラは入力 root 以下のソースを再帰的に読み込むため、互いに独立した例を同じ root に混在させないでください。

厳密な一括仕様は[言語仕様](../docs/language.md)、実装側の不変条件は[コンパイラ構成](../docs/architecture.md)、経緯は[機能の実装記録](../_features/README.md)にあります。サンプルの検証と更新方法は[文書の保守](contributing.md)を参照してください。

Microsoft Learn の [F# ドキュメント](https://learn.microsoft.com/ja-jp/dotnet/fsharp/)、[言語ガイド](https://learn.microsoft.com/ja-jp/dotnet/fsharp/language-reference/)、[ツアー](https://learn.microsoft.com/ja-jp/dotnet/fsharp/tour)の情報構成を参考にしています。本文とコード例は Tsuzuri の仕様に合わせたもので、F# の構文や .NET API の互換性を意味しません。
