# レコード、ジェネリックな型、型別名

[ドキュメントのトップ](../README.md)

レコードは名前付きフィールドを持つ不変の値です。フィールドの形が同じでも、異なる宣言やモジュールに属するレコードは異なる型です。

## 宣言と構築

```tsuzuri run=42
record Point { horizontal: i64, vertical: i64 }

def total :: ref Point -> i64 = \point -> point.horizontal + point.vertical

let point = Point { horizontal: 20, vertical: 22 }
total ref point
```

構築時には全フィールドをちょうど一回ずつ指定します。フィールドの記述順は宣言順と異なっていても構いません。参照を受け取る関数のフィールドアクセスは、自動的に参照先を読みます。

レコードはオブジェクト指向のクラスではなく、継承、仮想メソッド、可変フィールドを導入しません。操作はモジュールの関数として定義します。

## 所有権を使った更新

```tsuzuri run=42
record Position { horizontal: i64, vertical: i64 }

let original = Position { horizontal: 10, vertical: 20 }
let updated = { original with horizontal = 22 }
assert (original.horizontal == 10)
updated.horizontal + updated.vertical
```

`{ original with field = value; other = replacement }` は所有値を受け取り、指定フィールドを置換した新しい値を返します。レコードのフィールドへの代入ではありません。

元レコードを一度評価し、更新値を記述順にすべて評価してから結果を組み立てます。未指定フィールドは引き継ぎ、置換された旧フィールドを解放します。型と型引数は変更できません。

上のレコードは Copy なので元値も使えます。`string` などを含む非 Copy レコードでは元値が move され、更新後だけでなく更新値の計算中にも元の束縛を再使用できません。必要な値は更新前に計算します。

フィールドの重複は `E1001`、未知のフィールドは `E1007`、型不一致は `E1003` です。空の更新やフィールド名だけの省略記法はありません。

## ジェネリックなレコード

```tsuzuri run=42
record Pair<'left, 'right> { first: 'left, second: 'right }

def swap :: Pair<'left, 'right> -> Pair<'right, 'left> = \pair -> Pair { first: pair.second, second: pair.first }

let result: Pair<string, i64> = swap (Pair { first: 42, second: "answer" })
result.second
```

型引数はフィールド値や期待型から推論されます。各型パラメーターは少なくとも一つのフィールドで使い、重複や未宣言の型変数を認めません。

`Pair<i64, bool>` と `Pair<string, i64>` は異なる具体型です。前者は Copy、後者はフィールド単位に move できる非 Copy 型です。型パラメーターへ共有参照を渡す場合も、その所有者の寿命を保持します。排他参照を格納する具体化は拒否します。

比較、表示、既定値などは型クラスのインスタンスか `deriving` で追加します。ジェネリックな型にも、必要な制約を付けた条件付きインスタンスを定義できます。

## 型別名

```tsuzuri run=42
type Count = i64
type PairOf<'value> = 'value * 'value

def total :: PairOf<Count> -> Count = \pair ->
    match pair with
    | (left, right) -> left + right

total (20, 22)
```

`type Name = Type` は透過的な別名です。`Count` と `i64` は同じ型で、所有権、ABI、型クラスのインスタンスも同じです。単位や ID を型として区別したい場合は、単一 case の union を使います。

型別名の名前は ASCII 大文字で始めます。すべての型パラメーターを右辺で使う必要があります。循環した別名は `E1024`、展開上限の超過は `E1017` です。

別名は型注釈に使います。レコードリテラルとパターンには元のレコード名を使い、別名を新しい構築子として扱いません。インスタンスの重複も展開後の型で検査します。

## 公開範囲とホスト連携

`private record` / `private type` は宣言モジュール内だけで利用できます。公開 API から private 型を漏らすことはできません。

スカラーだけを持つレコードは対応 ABI を通じてホストへ渡せますが、言語内のレイアウトをそのまま C struct と見なしてはいけません。生成ヘッダーの正規化された形式を使います。

## 関連項目

- [型と型推論](types.md)
- [union](unions.md)
- [共有借用フィールドと lifetime](lifetimes.md)
