# 言語リファレンス

[ドキュメントのトップ](../README.md)

各記事に構文、実行例、評価順序、所有権、失敗条件、現在の制限をまとめています。初めて読む場合は[ツアー](../tour.md)、すべての契約を一括で確認する場合は[言語仕様](../../docs/language.md)を使います。

## 値と式

| 記事 | 内容 |
| --- | --- |
| [ソースとインデント](lexical-and-layout.md) | ファイル、識別子、予約語、コメント、空白 |
| [値と定数](values-and-constants.md) | let、mut、const、定数式 |
| [関数](functions.md) | def / fn、lambda、カリー化、捕捉、再帰 |
| [型と推論](types.md) | 基本型、タプル、型の構成、推論の境界 |
| [数値](numbers.md) | リテラル、折り返し、丸め、明示変換 |
| [文字列と文字](strings-and-characters.md) | UTF-16 / UTF-8、コード単位、スカラー |
| [演算子](expressions-and-operators.md) | 優先順位、結合、厳格評価、短絡 |

## データと多相性

| 記事 | 内容 |
| --- | --- |
| [レコードと型別名](records.md) | named data、型引数、所有更新、透過的別名 |
| [union と再帰型](unions.md) | case、payload、列挙型、所有ノード |
| [ジェネリック関数と型クラス](generics-and-typeclasses.md) | 制約、条件付き instance、default、比較 |
| [高階型](higher-kinds.md) | kind、型コンストラクター、末尾固定 |
| [自動導出](deriving.md) | Eq、Ord、Display、Hash、Default |

## 所有権と制御

| 記事 | 内容 |
| --- | --- |
| [所有権と記憶域](ownership.md) | move、Copy、ref、ref mut、new、drop |
| [名前付き lifetime](lifetimes.md) | 返却元、借用フィールド、region の制限 |
| [条件とループ](control-flow.md) | if、for、while、範囲、break / continue |
| [パターンマッチ](patterns.md) | 分解、網羅性、OR / AND、ガード |
| [アクティブパターン](active-patterns.md) | 全域、部分、Option、複数 case |
| [コンピュテーション式](computation-expressions.md) | .tc、Bind、Delay、match! / and! |
| [タスク](tasks.md) | cold な一回実行、並列、Result、寿命 |
| [モジュールとパッケージ](modules-and-packages.md) | root、修飾名、private、std、path 依存 |

## キーワードから探す

| キーワード | 主な説明 |
| --- | --- |
| def / fn / fx / rec / and | [関数](functions.md) |
| let / mut / const | [値と定数](values-and-constants.md) |
| record / type / with | [レコード](records.md) |
| union / of | [union](unions.md) |
| class / instance | [型クラス](generics-and-typeclasses.md) |
| deriving | [自動導出](deriving.md) |
| ref / deref / new | [所有権](ownership.md) |
| if / then / elif / else / for / in / to / downto / while / do / break / continue | [制御構文](control-flow.md) |
| match / when / as | [パターン](patterns.md)。数値の as は[変換](numbers.md) |
| return / yield / let! / do! / match! / and! | [計算式](computation-expressions.md) |
| task | [Task](tasks.md) |
| private | [モジュール](modules-and-packages.md) |
| export / extern | [ホスト連携](../guides/native-interop.md) |
| test | [言語内テスト](../tools/testing.md) |
| true / false | [型](types.md) |

コレクションの構文・スライスと API は[標準ライブラリ](../library-reference/README.md)、CLI と診断は[ツール](../tools/README.md)、段階的な対応範囲は[機能別索引](../feature-status.md)を参照してください。
