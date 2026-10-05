# ジェネリック関数と型クラス

[ドキュメントのトップ](../README.md)

ジェネリック関数は型を変えて同じ処理を使う仕組みです。型クラスは、その型で利用できる演算を指定します。型クラスのインスタンスはコンパイル時に選ばれ、実行時の辞書や仮想ディスパッチは生成しません。

## 型変数と制約

```tsuzuri run=42
def twice :: (Add<'value>, Copy<'value>) => 'value -> 'value = \value -> value + value

twice 21
```

`'value` はシグネチャで全称量化される型変数です。上の本体には加算と複製が必要なので、`Add` と `Copy` を要求します。本体や呼び出す関数から制約を推論することもできます。

制約には三つの表記があります。

```text
def add :: Add<'value> -> 'value -> 'value
def add :: Add<'value> => 'value -> 'value -> 'value
def add :: 'value -> 'value -> 'value
    @'value : Add
```

これらは説明用の別々の宣言です。同じ関数の重複宣言として同じファイルに並べるものではありません。`Add<'value>` を引数位置に書く形式は、新しいデータ型ではなく制約付きの `'value` です。

`@` 行は宣言より深く、複数行なら同じ深さにそろえます。対象はシグネチャに現れる型変数です。利用者定義の型クラスはモジュール名で修飾できます。

## 型クラスの宣言

次の例は二つのファイルで構成します。`Traits.tt`:

```tsuzuri project=traits file=Traits.tt
class Score<'value> {
    def score :: ref 'value -> i64
}

class Eq<'value> => Comparable<'value> {
    def same :: ref 'value -> ref 'value -> bool = \left right -> Eq.eq left right
}
```

同じ root の `Main.tz`:

```tsuzuri project=traits file=Main.tz run=42
record Amount { value: i64 } deriving (Eq)

instance Traits.Score<Amount> {
    fn score amount = amount.value
}

instance Traits.Comparable<Amount> {}

let amount = Amount { value: 42 }
let other = Amount { value: 42 }
assert (Traits.Comparable.same (ref amount) (ref other))
Traits.Score.score ref amount
```

型クラスは `.tt`、レコードとインスタンスは `.tz` / `.tc` に置きます。インスタンスのメソッド型はクラスから取得するので、`def` を再記述しません。

`Comparable` は `Eq` をスーパークラスとして要求し、`same` のデフォルト実装を提供しています。デフォルトがあるメソッドはインスタンス側で省略できます。スーパークラスの循環は許可しません。

## 条件付きインスタンス

```tsuzuri run=42
record Box<'value> { value: 'value }

instance Eq<'value> => Eq<Box<'value>> {
    fn eq left right = left.value == right.value
    fn ne left right = !(Eq.eq left right)
}

let left = Box { value: "answer" }
let right = Box { value: "answer" }
assert (left == right)
42
```

`Eq<Box<T>>` を使うには `Eq<T>` が必要です。この必要条件は呼び出し元へ伝播します。インスタンスの本体が追加の制約を必要とする場合、宣言した context に含めます。

同じクラスで head が単一化できるインスタンスは併存できません。例えば汎用の `Eq<Box<'value>>` と特殊な `Eq<Box<i64>>` を定義して「具体的な方を優先」することはありません。ファイル順にも依存せず、重複は `E1016` です。

## 組み込みクラス

| クラス | 主な用途 |
| --- | --- |
| `Add`, `Sub`, `Mul`, `Div`, `Rem`, `Neg` | 算術演算。`Rem` は整数のみ |
| `Eq`, `Ord` | 共有借用による等値・順序比較。`Ord` は `Eq` を要求 |
| `Bits` | 整数のビット論理、シフト、反転 |
| `Integer`, `SignedInteger`, `UnsignedInteger` | 整数型の分類 |
| `Float`, `Numeric` | 浮動小数点、数値型の分類 |
| `Elementary` | f32 / f64 の超越関数 |
| `Copy` | 所有値を複製できること |
| `Capture` | 再利用可能な関数環境へ保存できること |
| `Send` | タスクへ所有値を渡せること |
| `Display` | `display :: ref T -> string` |
| `Parse` | `parse :: ref string -> Option<T>` |
| `Hash` | `hash :: ref T -> i64u` |
| `Default` | `default :: T`。`Default.default()` で呼ぶ |
| `Drop` | `drop :: ref mut T -> unit`。値の終わりに一度だけ自動で呼ばれる（[利用者定義の解放](ownership.md#利用者定義の解放drop)） |

メソッドを持たない組み込み分類クラスへ利用者のインスタンスは追加できません。`Capture` は排他参照や Task を含む型を拒否し、`Send` は格納された借用を含む型を拒否します。関数の引数型に参照があることと、関数値が参照を捕捉することは別に検査します。`Drop` のインスタンスは、利用者が宣言した record・union に全ての型引数を型変数で書いた head（`Drop<Handle<'a>>`）だけで、制約や `deriving (Drop)` は使えません。

## 比較は非消費

`==`, `!=`, `<`, `<=`, `>`, `>=` は値を共有借用して比較します。文字列や非 Copy の独自型も比較後に使えます。比較中の所有者の変更・move・排他借用は拒否します。

`Eq.eq` / `Ord.lt` などのメソッドは共有参照を取ります。算術メソッドは値を取るため、比較と同じ所有権契約ではありません。比較演算子自体が任意の参照を自動的に外すわけではなく、参照引数同士を比べるときはメソッド形式が明確です。

配列・リスト・タプルの構造比較は、必要な要素の `Eq` / `Ord` を要求します。順序比較は辞書式で短絡し、比較のためにコレクション全体を複製しません。レコードや union は手書きのインスタンスまたは自動導出を使います。

## 定義元モジュールの関数を要求する

`Point.tz`:

```tsuzuri project=module-constraint file=Point.tz
record Point { horizontal: i64, vertical: i64 }
def total :: Point -> i64 = \point -> point.horizontal + point.vertical
```

`Main.tz`:

```tsuzuri project=module-constraint file=Main.tz run=42
def total_of :: 'value -> 'result
    @'value : #total = \value -> 'value.total value

total_of (Point { horizontal: 20, vertical: 22 })
```

`#total` はレコード・union の定義元モジュールに `total` という関数があることを要求します。クラスのインスタンス登録は不要で、関数の型、可視性、追加制約は通常どおり検査します。

対象は定義元モジュールを持つレコード・union です。プリミティブや配列に任意の同名関数を探索する仕組みではなく、private も迂回しません。`'value.total` の使用には明示的な `#total` が必要です。

## 制限

通常の型クラスは一つの型パラメーターを持ちます。高ランク多相、通常 kind のメソッド固有型変数、メソッド固有の制約は未対応です。高階型クラスには別の対応範囲があります。

単相化は具体型ごとにコードを作るため、型の増大が続く再帰には制限があります。追加特殊化 1,024、型深さ 128、型構成要素 4,096、関数へ伝播する各種制約 128 などの上限を持ち、超過は `E1017` になります。

## 関連項目

- [高階型](higher-kinds.md)
- [自動導出](deriving.md)
- [関数と単相なローカル値](functions.md)
