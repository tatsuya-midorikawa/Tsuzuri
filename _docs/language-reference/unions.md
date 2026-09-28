# 判別共用体と再帰的なデータ型

[ドキュメントのトップ](../README.md)

`union` は複数の形のうち一つを持つ値です。状態遷移、成功と失敗、木構造など、取り得る状態を型で列挙したい場合に使います。

## 宣言と case

```tsuzuri run=42
union Measurement =
    | Missing
    | Single of i64
    | Pair of i64 * i64

def total :: Measurement -> i64 = \measurement ->
    match measurement with
    | Missing -> 0
    | Single value -> value
    | Pair (left, right) -> left + right

total (Pair (20, 22))
```

case の payload は 0 個または 1 個です。複数の値を持たせたいときはタプルやレコードを一つの payload にします。payload なしの `Missing` は値、payload 付きの `Single` は一引数の関数値としても使えます。

union 名と case 名は ASCII 大文字で始めます。同じモジュールでは型名・case 名・関数名などの衝突を認めず、union 自身と同じ名前の case も使えません。例えば ID を包むなら `union UserId = UserIdValue of i64` と区別します。

## 列挙型と汎用型

全 case に payload がなければ列挙型として使えます。case に任意の整数値を割り当てたり、整数と暗黙に相互変換したりする機能ではありません。

```tsuzuri run=42
union Choice<'value> = Absent | Present of 'value

def value_or :: 'value -> Choice<'value> -> 'value = \fallback choice ->
    match choice with
    | Absent -> fallback
    | Present value -> value

value_or 0 (Present 42)
```

各型パラメーターはいずれかの payload で使います。標準の `Option<T>` / `Result<T, E>` も union です。一般的な「値なし」や失敗を表す場合は、独自型より標準型を優先すると API を組み合わせやすくなります。

## 名前の解決

case は `Module.Case`、自モジュールの `Union.Case`、`Module.Union.Case` で修飾できます。無修飾では自モジュールを優先し、他モジュールの候補が曖昧なら修飾が必要です。

パターンの名前は case、アクティブパターン、変数束縛の順に解決します。payload の個数が合わない場合は `E1020`、`match` で必要な case が不足する場合は `E1021` です。

## 所有権

非再帰 union は全 payload が Copy なら Copy です。所有する union の非 Copy payload をパターンで受け取ると move され、元の union 全体を再使用できません。

共有参照やコレクションの要素を照合する場合、非 Copy payload は読み取り専用のビューになることがあります。そのビューを借用することはできますが、所有値として持ち去ることはできません。ガードの束縛も読み取り専用なので、ガードが失敗した後の節で同じ入力を照合できます。

`Option<ref T>` のような具体化は所有者の借用寿命を保持します。ただし直接の `Hold of ref T` のような借用 payload 宣言や排他参照の格納は、対応範囲に入りません。

## 再帰する型

```tsuzuri run=42
union Tree = Empty | Node of Tree * i64 * Tree

def rec sum :: Tree -> i64 = \tree ->
    match tree with
    | Empty -> 0
    | Node (left, value, right) -> sum left + value + sum right

sum (Node (Node (Empty, 20, Empty), 22, Empty))
```

union の payload を通る循環を許可します。`record Link { next: Option<Link> }` も有限の終端を作れます。空の配列・リスト・Vec を使って有限値を作れる形もあります。

union を通らない直接のレコード循環や、有限値を作れない `union Bad = Loop of Bad` は `E1010` です。型引数が増大・入れ替わり続ける再帰は `E1017` になります。

再帰する具体型は非 Copy で、ノードは暗黙にヒープへ確保されます。`new` や `box` を追加する必要はありません。最初の payload なし case は確保なしで表せます。別の具体化が再帰しなければ、その具体化まで再帰ノード表現にはなりません。

## 深い構造の注意点

再帰ノードの解放と捕捉環境用の深い複製は反復走査であり、木の深さに比例する実行スタックを使いません。ノードを共有する GC グラフや循環参照を導入するものではありません。

上の `sum` のような利用者の非末尾再帰、導出した比較・表示・Hash の走査は一般の関数呼び出しです。深い構造でのスタック安全性は別に考える必要があります。

union は直接のホスト ABI には出せません。ホストへ渡すときは、対応するスカラーやバッファへ変換する公開関数を設けます。

## 関連項目

- [型の選択と推論](types.md)
- [レコードと型別名](records.md)
- [所有権](ownership.md)
