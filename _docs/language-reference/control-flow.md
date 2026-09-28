# 条件分岐とループ

[ドキュメントのトップ](../README.md)

条件分岐もループも式です。`if` は選んだ分岐の値、`for` / `while` は `unit` を返します。通常のループは逐次実行で、自動的なスレッド起動や GPU 転送はありません。

## if、elif、else

```tsuzuri run=42
def clamp :: i64 -> i64 = \value ->
    if value < 0 then 0
    elif value > 42 then 42
    else value

clamp 100
```

条件は bool、結果の各分岐は同じ型です。未選択の分岐は実行しませんが、型検査はします。`else` を省くと未選択側は `()` となり、選択側も unit にする必要があります。

`else if` も使えます。`if condition { value } else { other }` の明示ブロック形式も有効です。条件内にレコードリテラルを置くときは括弧で囲みます。

## for to と downto

```tsuzuri run=6
let mut total: i64 = 0
for index = 1 to 3 do
    total = total + index as i64
total
```

`for name = start to finish do ...` は i32 の昇順、`downto` は i32 の降順です。開始と終了を左から右に一度ずつ評価し、両端を含みます。方向に対して空なら一度も本体を実行しません。

ループ変数はそのループ内の不変束縛です。終点が i32 の最小値・最大値でも、最後の反復で終了し、カウンターの折り返しによる無限ループにはなりません。

## 整数範囲

```tsuzuri run=252
let mut total: i64 = 0
for value in 125i8 .. 2i8 .. 127i8 do
    total = total + value as i64
total
```

`start .. finish` と `start .. step .. finish` は、8-bit から 128-bit の整数に対応します。三つの値は同じ型で、開始・刻み・終了の順に一度ずつ評価します。省略した刻みは 1 です。

終点を越えない値を列挙し、刻みが負なら降順です。刻み 0 はトラップです。次の値が整数型の範囲外になる場合はそこで終了し、折り返しません。

これは `for ... in` の列挙構文であり、保存できる汎用シーケンス値や浮動小数点範囲ではありません。配列の添字を列挙する場合、終端も含むので `0 .. (length - 1)` とします。

## コレクションの反復

配列・リストは格納順、string は UTF-16 コード単位、utf8string は UTF-8 バイトで列挙します。Seq の反復と `Module.iter` で作る借用イテレーターも利用できます。

通常の配列・リストの for は列挙のために全体を複製せず、列挙元を読み取り借用します。ループ中の元値の move・置換・排他借用は拒否されます。非 Copy 要素は借用して読めますが、所有値として持ち去れません。

タプルなどの分解パターンも使えます。ただし for のパターンは網羅性検査されず、不一致の要素はスキップではなくトラップです。

## while、break、continue

```tsuzuri run=9
let mut total = 0
for value in 1 .. 10 do
    if value > 5 then break
    if value % 2 == 0 then continue
    total = total + value
total
```

`while condition do body` は反復の前に毎回 bool の条件を評価します。`break` は最内の通常ループを終了し、`continue` は次の反復へ進みます。範囲の continue でも終端・オーバーフロー検査は維持します。

ジャンプは unit 型で、値付き・ラベル付きの形式はありません。外側の lambda、task、ビルダーの境界を越えて脱出できず、不正な場所では `E1023` です。カスタムビルダーの `For` / `While` 自体へのジャンプにも使えません。

通常終了やジャンプで不要になる所有値は解放します。実行されない後続コードも型・所有権検査からは除外されません。外側の非 Copy 値を毎反復 move する場合は、次の反復までに再初期化できる必要があります。

## 関連項目

- [パターンマッチとガード](patterns.md)
- [コンピュテーション式](computation-expressions.md)
- [所有権と借用](ownership.md)
