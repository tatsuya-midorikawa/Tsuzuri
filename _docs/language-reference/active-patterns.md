# アクティブパターン

[ドキュメントのトップ](../README.md)

アクティブパターンは、関数で入力を判定・変換してからパターン照合する仕組みです。型付きの通常の関数として宣言し、専用の動的ディスパッチは使いません。

## bool を返す部分パターン

```tsuzuri run=42
def (|Even|_|) :: i64 -> bool
fn (|Even|_|) value = value % 2 == 0

match 42 with
| Even -> 42
| _ -> 0
```

`(|Name|_|)` は不一致になり得る認識器です。bool 形式では true なら一致し、payload のパターンを付けません。false なら次の節へ進みます。

## Option を返す部分パターン

```tsuzuri run=42
def (|Parsed|_|) :: ref string -> Option<i64>
fn (|Parsed|_|) text = Parse.parse text

match "42" with
| Parsed value -> value
| _ -> 0
```

`Some payload` なら payload を後続のパターンへ渡し、`None` なら不一致です。`Option<unit>` の場合だけ payload パターンを省略できます。

読み取りだけの入力には `ref string` のような借用を使います。非 Copy の入力を消費してから、同じ入力で別の節を試すことはできません。

## 全域パターン

```tsuzuri run=42
def (|Length|) :: ref string -> i64
fn (|Length|) text = text.length

match "answer" with
| Length size -> size * 7
```

`(|Name|)` は常に認識結果を返します。網羅性上は、結果側のパターンが単独で網羅的な場合だけ入力全体を覆います。`Length 5` は全域認識器の使用でも、他の長さを覆いません。

`Parity true` と `Parity false` を別々の節に書いた場合も、認識器の結果をまたいだ合成で網羅性を証明するわけではありません。

## 追加引数

```tsuzuri run=42
def (|Divisible|_|) :: i64 -> i64 -> bool
fn (|Divisible|_|) divisor value = value % divisor == 0

match 42 with
| Divisible 3 -> 42
| _ -> 0
```

追加引数は照合対象より前に認識器へ渡します。全域形式では追加引数の後に結果のパターンを続けます。追加引数も通常の評価順序・トラップ規則に従います。

## 複数 case の全域パターン

```tsuzuri run=42
union ParityValue = EvenValue | OddValue

def (|Even|Odd|) :: i64 -> ParityValue
fn (|Even|Odd|) value = if value % 2 == 0 then EvenValue else OddValue

match 42 with
| Even -> 42
| Odd -> 0
| _ -> -1
```

戻り値には認識 case と同じ個数の case を持つ明示的な union を使います。両者は宣言順に対応します。同じモジュールでは名前が衝突しないよう、上の `Even` と `EvenValue` のように区別します。

隠し union は生成しません。payload 付き case は対応する payload パターンを要求します。`(|First|Second|_|)` のように複数 case と部分形式を組み合わせることはできません。

認識器は節ごとに再評価されるため、現在は全 case を並べても `_` のフォールバックが必要です。認識結果を勝手にキャッシュして副作用や評価順を変えることはありません。

## 名前、可視性、診断

認識 case は大文字で始めます。他モジュールの候補が曖昧なら `Module.Name` で修飾し、private な認識器は外から使用できません。定義の case 数不一致は `E1020`、名前衝突は `E1001` です。

match に加え、fx の引数や for の分解にも使えます。ただし後者の不一致はトラップです。null や .NET の実行時型テストを提供する仕組みではありません。

## 関連項目

- [パターンと網羅性](patterns.md)
- [関数](functions.md)
- [union](unions.md)
