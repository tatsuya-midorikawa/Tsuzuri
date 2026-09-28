# アクティブパターン

[ドキュメントのトップ](../README.md)

アクティブパターンは、関数で入力を判定・変換してからパターン照合する仕組みです。型付きの通常の関数として宣言し、専用の動的ディスパッチは使いません。

## bool を返す部分パターン

```tsuzuri run=42
def (|Even|_|) :: i64 -> bool = \value -> value % 2 == 0

match 42 with
| Even -> 42
| _ -> 0
```

`(|Name|_|)` は不一致になり得る認識器です。bool 形式では true なら一致し、payload のパターンを付けません。false なら次の節へ進みます。

## Option を返す部分パターン

```tsuzuri run=42
def (|Parsed|_|) :: ref string -> Option<i64> = \text -> Parse.parse text

match "42" with
| Parsed value -> value
| _ -> 0
```

`Some payload` なら payload を後続のパターンへ渡し、`None` なら不一致です。`Option<unit>` の場合だけ payload パターンを省略できます。

読み取りだけの入力には `ref string` のような借用を使います。非 Copy の入力を消費してから、同じ入力で別の節を試すことはできません。

## 全域パターン

```tsuzuri run=42
def (|Length|) :: ref string -> i64 = \text -> text.length

match "answer" with
| Length size -> size * 7
```

`(|Name|)` は常に認識結果を返します。網羅性上は、結果側のパターンが単独で網羅的な場合だけ入力全体を覆います。`Length 5` は全域認識器の使用でも、他の長さを覆いません。

`Parity true` と `Parity false` を別々の節に書いた場合も、認識器の結果をまたいだ合成で網羅性を証明するわけではありません。

## 追加引数

```tsuzuri run=42
def (|Divisible|_|) :: i64 -> i64 -> bool = \divisor value -> value % divisor == 0

match 42 with
| Divisible 3 -> 42
| _ -> 0
```

追加引数は照合対象より前に認識器へ渡します。全域形式では追加引数の後に結果のパターンを続けます。追加引数も通常の評価順序・トラップ規則に従います。

## 複数 case の全域パターン

```tsuzuri run=42
def (|Even|Odd|) :: i64 -> 'T = \value ->
    | value % 2 == 0 -> Even
    | otherwise -> Odd

match 42 with
| Even -> 42
| Odd -> 0
| _ -> -1
```

戻り値の `'T` は、この認識器専用の union を暗黙生成する印です。通常の汎用型変数ではなく、入力型や制約には使いません。明示的な union と別名の case は不要です。本体内では、宣言した case 名を結果の値として使います。

認識器本体でも[関数ガード](patterns.md#関数ガード)と後置の `where` を使えます。payload 付きの case や部分パターンでも同じです。bool 条件だけなら `otherwise` などの無条件の節が必要です。

payload の型は本体から推論します。

```tsuzuri run=42
def (|Small|Large|) :: i64 -> 'T = \value ->
    if value < 10 then Small value else Large value

match 42 with
| Small value -> value
| Large value -> value
| _ -> 0
```

本体で `Case value` または `value |> Case` と直接適用した case は payload を持ちます。間接適用だけでは payload の有無を推論しません。payload 付き case は対応する payload パターンを要求します。本体から型が決まらない場合は値に型注釈を付けます。`(|First|Second|_|)` のように複数 case と部分形式を組み合わせることはできません。

明示的な union 型を返す旧形式と、複数 case の `def` / `fn` を分離する旧形式は廃止しました。`def (|First|Second|) :: 入力型 -> 'T = ラムダ式` へ移行します。

認識器は節ごとに再評価されるため、現在は全 case を並べても `_` のフォールバックが必要です。認識結果を勝手にキャッシュして副作用や評価順を変えることはありません。

## 名前、可視性、診断

認識 case は大文字で始めます。他モジュールの候補が曖昧なら `Module.Name` で修飾し、private な認識器は外から使用できません。不正な複数 case 形式は `E1020`、名前衝突は `E1001` です。結果を作る case 名は認識器本体内だけの束縛で、外側では認識パターンとして使います。

match に加え、ラムダ式の引数や for の分解にも使えます。ただし後者の不一致はトラップです。null や .NET の実行時型テストを提供する仕組みではありません。

## 関連項目

- [パターンと網羅性](patterns.md)
- [関数](functions.md)
- [union](unions.md)
