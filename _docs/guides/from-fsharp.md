# F# に慣れた利用者のための Tsuzuri

[ドキュメントのトップ](../README.md)

関数の空白適用、パイプ、パターンマッチ、コンピュテーション式などの考え方は似ていますが、Tsuzuri は F# の方言や .NET のフロントエンドではありません。コードを移すときは、表記だけでなく評価時点と所有権を見直します。

## 構文の対応

| 意図 | F# の代表的な表記 | Tsuzuri |
| --- | --- | --- |
| 名前付き関数 | `let add left right = ...` | def に型を宣言し、fn または対応する let で実装 |
| 匿名関数 | `fun value -> ...` | `value -> ...` または `fx value -> ...` |
| 可変ローカル | `let mutable count = 0` | `let mut count = 0` |
| 代入 | `<-` | `=` |
| 等値・不等値 | `=` / `<>` | `==` / `!=` |
| 配列 | `[\|1; 2\|]` | `[1, 2]` |
| リスト | `[1; 2]` | `[\|1, 2\|]` |
| 判別共用体 | type に case を列挙 | union に case を列挙 |
| レコード | type とフィールド | record と明示的な型 |
| モジュール | module / namespace / open | ファイルとディレクトリ、修飾名 |
| 関数合成 | `>>` / `<<` | これらは整数シフト。合成は通常の関数で記述 |

上表は代表的な記法の比較で、機械的な全構文変換表ではありません。

## 型推論と多相性

Tsuzuri の名前付き関数はシグネチャを明示します。ローカル let は単相で、すべての関数値を自動一般化するわけではありません。整数の既定は i64、小数は f64 です。

型クラスで演算を抽象化し、コンパイル時の単相化で具体化します。class というキーワードは .NET オブジェクトの class 宣言ではありません。継承、仮想メソッド、リフレクションを意味しません。

## 所有権を API に表す

F# の GC で管理する値の共有を、Tsuzuri の値渡しへそのまま移すことはできません。string は非 Copy で move し、Copy 配列を複製すれば独立したバッファになります。

```tsuzuri run=10
def size :: ref string -> i64
fn size text = text.length

let text = "hello"
size text + size text
```

ref は F# の参照セルではなく共有借用です。ref mut は排他借用であり、複数箇所から同じ可変状態を自由に変更する許可ではありません。関数の引数型が分かれば借用を省略できても、寿命と競合は検査します。

クロージャーが捕捉する値はスナップショットです。外側の let mut を関数内から変更する一般的なパターンは使えません。必要な状態は引数と返却値で明示的に受け渡します。

## Option、Result、例外

Option / Result と match は利用できます。match の網羅性不足はコンパイルエラーです。get の不一致、assert、整数ゼロ除算、境界違反はトラップであり、F# の例外のように try / with で回復しません。

use / try / finally、任意 destructor の呼び出し、.NET の IDisposable は未対応です。所有値の lexical な解放と、回復可能な失敗を表す Result を使います。

## 計算式とタスク

ビルダーはオブジェクトではなく .tc のモジュールです。Return / Bind などはカリー化された通常の関数で、Zero は引数なしです。Option / Result の For は Copy 要素の配列を受けます。

and! は右辺を順に評価して結合し、自動並列起動しません。task 内の and! は提供しません。並列化には Task.parallel を明示します。

Tsuzuri の task は cold・一回実行・非 Copy です。Task.run は同期的で、F# の通常の hot task や .NET Task の scheduler / awaiter との互換はありません。

Seq も一回消費です。`seq { ... }` という組み込みビルダーや再列挙可能な IEnumerable の代わりと決めつけず、Seq.unfold / defer や明示 iter を使います。

## 数値と文字列

通常の整数演算は折り返し、浮動小数点から整数への変換は飽和と NaN から 0 です。整数同士の cast は bit 保持・切り詰め・拡張です。F# / .NET の変換や checked 文脈と同一視しません。

string は UTF-16 で、添字の返却型は i16u です。utf8string は別型で、索引はバイトです。文字列補間、printf 風の書式、.NET String の全メソッド、文化圏依存比較はありません。

## 提供しない F# / .NET 機能

オブジェクト指向の class / interface / 継承、型プロバイダー、単位付き型、コードクォート、属性、reflection、LINQ query、null、F# Interactive を前提とするコードは直接移植できません。

Tsuzuri が担当するのは型付きの計算と所有する状態です。外部機能は C / WASM ホストへ置き、export / extern の ABI を通して接続します。

## 関連項目

- [所有権](../language-reference/ownership.md)
- [API 設計とスタイル](style-and-design.md)
- [対応状況](../feature-status.md)
