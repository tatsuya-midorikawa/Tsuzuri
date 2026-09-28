# 比較、表示、Hash、既定値の自動導出

[ドキュメントのトップ](../README.md)

`deriving` はレコードや union の構造から型クラスのインスタンスを作ります。標準的な構造比較などを利用したい場合に使い、独自の意味を持つ比較や表示は手書きのインスタンスにします。

## 基本例

```tsuzuri run=42
record Point { horizontal: i64, vertical: i64 } deriving (Eq, Ord, Display, Hash, Default)

let point = Point { horizontal: 20, vertical: 22 }
let other = Point { horizontal: 20, vertical: 22 }
let zero: Point = Default.default()
let shown = Display.display ref point
assert (point == other)
assert (zero < point)
assert (shown == "Point { horizontal: 20, vertical: 22 }")
assert (Hash.hash (ref point) == Hash.hash (ref other))
point.horizontal + point.vertical
```

指定できるクラスは `Eq`, `Ord`, `Display`, `Hash`, `Default` です。`deriving` は宣言直後に置き、同じモジュールに通常の条件付きインスタンスを生成します。

ジェネリックな型では必要な要素の制約を要求します。導出できないフィールドやクラスは `E1025`、手書きのインスタンスとの重複は `E1016` です。

## 比較の順序

`Eq` はレコードのフィールド順に比較し、最初の不一致で終了します。union は case が等しいかを確認してから payload を比較します。

`Ord` は別途 `Eq` を要求します。レコードはフィールドの辞書式順序、union は case の宣言順で比較します。case 名の辞書順ではありません。

浮動小数点の NaN の意味も変えません。最初の不一致で大小比較が false なら、その後のフィールドで順序を決め直すことはありません。

## 既定値

```tsuzuri run=42
union Status = Waiting | Completed of i64 deriving (Eq, Display, Default)

let initial: Status = Default.default()
assert (initial == Waiting)
let result = Completed 42
match result with
| Waiting -> 0
| Completed value -> value
```

レコードでは各フィールドの既定値、union では最初の case の既定値を作ります。空コレクションを作るために要素型の Default は必要ありません。最初の case が再帰し続け、有限の既定値を作れない定義は拒否します。

## 表示

レコードは `Point { horizontal: 20, vertical: 22 }`、union は `Waiting` または `Completed 42` のように表示します。配列は `[a, b]`、リストは `[|a, b|]`、タプルは `(a, b)` です。

構造内の文字列や文字はリテラル風に引用し、UTF-8 型には `u8` 接頭辞を付けます。引用符、逆斜線、改行などをエスケープし、孤立サロゲートも表示可能な表記にします。単体の文字列を Display する場合の raw 出力とは区別してください。

Display の出力を汎用シリアライザーや Parse の自動逆変換と見なすことはできません。

## Hash の契約

Hash は決定的な 64-bit FNV-1a です。offset は `14695981039346656037`、prime は `1099511628211` で、各呼び出しは同じ offset から始めます。ポインターやパディングは入力に含めません。

タグ、長さ、要素 index、子の Hash は 8-byte little-endian で混ぜます。整数・浮動小数点の payload は型幅で、文字列は UTF-16 コード単位または UTF-8 バイトで処理します。

| 対象 | 正規化 |
| --- | --- |
| 正負のゼロ | 同じ Hash |
| NaN | 型ごとの正の quiet NaN にそろえる |
| decimal の等しい cohort | 係数末尾のゼロを除いて正規化 |
| 配列・リスト・タプル・レコード | 個数、index、各要素の Hash を順序付きで合成 |
| union | 宣言順の case index と payload の Hash |

等しい Hash は等しい値を保証しません。ランダム seed はなく、暗号用途や HashDoS 対策には使えません。バイト単位の形式は[正式な Hash ストリーム仕様](../../docs/language.md#自動導出deriving)を参照してください。

## 再帰型での注意

再帰型も導出できますが、比較、表示、Hash は一般の再帰メソッドです。深い木に対してスタックを消費する可能性があります。再帰ノードの解放・複製の反復実装とは別の保証です。

生成インスタンス数や生成式にはコンパイラの資源上限があり、超過は `E1017` です。

## 関連項目

- [型クラス](generics-and-typeclasses.md)
- [再帰的な union](unions.md)
- [レコード](records.md)
