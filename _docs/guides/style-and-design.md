# API 設計とコーディングスタイル

[ドキュメントのトップ](../README.md)

読み手が型、評価順序、所有権、失敗を局所的に追える構成にします。構文上使える機能を増やすことと、利用者が必要とする API を簡単にすることは別です。

## 名前とレイアウト

関数とローカル値は snake_case、型とモジュールは読み分けやすい大文字始まりを推奨します。識別子は ASCII です。型名や case 名に大文字始まりが必須な規則と、関数名の推奨を区別します。

通常のコード例とサンプルは `def name :: 型 = \引数 -> 本体` を基本にします。引数なしの関数は `def name :: 型 = 本体` です。分離した `def` / `fn` / `let` は構文の比較説明で使い、型クラスの instance など署名を継承する実装はその専用形式に従います。

4 スペースのインデントを使い、複雑な式は let で分けます。空白適用が改行をまたがないこと、明示ブロックの let はセミコロン区切りであることを意識します。機械的な空白は fmt で統一します。

## 読み取り、消費、更新を分ける

読み取りだけなら ref T、所有権を取得するなら T、値全体を置換するなら ref mut T を API で区別します。大きな Copy 型でも、値渡しの複製が安いとは限りません。

```tsuzuri run=42
record Counter { value: i64 }

def current :: ref Counter -> i64 = \counter -> counter.value

def increment :: Counter -> Counter = \counter ->
    let next = counter.value + 1
    { counter with value = next }

let counter = Counter { value: 41 }
let updated = increment counter
current ref updated
```

所有更新の前に必要な値を計算すれば、非 Copy フィールドが後から追加されても、元値の move 中に読み直す構造を避けやすくなります。公開関数の所有権契約は実装上の都合だけで変更しないでください。

## 不正な状態を型で分ける

複数の状態には union、名前のある複数項目には record、一時的な複数結果には tuple を使います。型別名は別型ではないため、区別したい ID や単位には単一 case の union を選びます。

値がない場合は Option、理由付きの失敗は Result を使います。ユーザー入力を get や assert で無条件に受け入れるのではなく、失敗値を処理します。トラップ後の回復や destructor を前提に設計しません。

## コレクションを選ぶ

| 用途 | 候補 |
| --- | --- |
| 密な連続データ、一括処理 | Array |
| 長さが増減する構築段階 | Vec |
| 先頭追加と順次走査 | List。ランダムアクセスを多用しない |
| キー順の検索と列挙 | Map / Set。更新 O(n) を考慮 |
| 一回消費する遅延生成 | Seq |
| コピーなしの部分読み取り | ref [T] のスライス |

非 Copy 要素の処理は map_ref / fold_ref や iter を使います。API によって callback の引数順が違うため、F# の慣習だけで順序を推測しないでください。

## 抽象化とモジュール

一つの型に関する操作を定義元モジュールへ集めます。公開する必要のない helper は private にし、型クラスは複数型に共通の操作が必要になったときに導入します。

HKT や独自ビルダーは通常の Option.map / Result.bind などで十分な処理に必須ではありません。小さい関数と標準 API で契約を明確にできるなら、それを優先します。

## ホスト境界

extern は副作用と信頼境界を持ちます。ホストは借用入力を保持せず、返却バッファの allocator と所有権を守ります。並列に呼ぶ場合は thread safety を確認します。

WASM の i64 は BigInt、pointer は Number です。view の lifetime、memory.grow、UTF-16 / UTF-8 の長さ単位を API の説明に含めます。

## テストと性能

通常値に加え、空入力、範囲の両端、overflow、NaN、符号付きゼロ、非 Copy 要素、借用の寿命を検証します。実行経路を変更する場合は native / WASM と O0 / O3 の意味の一致を確認します。

性能のために意味を変える場合は暗黙最適化ではなく別 API の契約にします。並列化や GPU を選ぶ前に、コピー、確保、同期、転送を含めた費用を測定します。

## 関連項目

- [関数](../language-reference/functions.md)
- [所有権](../language-reference/ownership.md)
- [性能ガイド](performance.md)
- [テスト](../tools/testing.md)
