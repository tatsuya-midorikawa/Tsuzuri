# 表示、解析、文字列化

[ドキュメントのトップ](../README.md)

Display は値から UTF-16 string を作り、Parse は string から値を解析します。locale に依存しない共通実装を使い、native と WASM で同じ表現になります。

## Display と to_string

```tsuzuri run=42
let text = to_string 42
let parsed: Option<i64> = Parse.parse ref text
Option.get parsed
```

| API | 所有権 |
| --- | --- |
| `Display.display value` | `Display<T> => ref T -> string`。入力を借用 |
| `to_string value` | `Display<T> => T -> string`。入力を消費 |
| `Parse.parse text` | `Parse<T> => ref string -> Option<T>`。入力を借用 |

string の to_string は元のバッファをそのまま返せます。借用版 Display は独立した複製を返します。utf8string の表示は UTF-16 への変換を行います。単体の文字列の NUL、改行、引用符はそのままで、JSON などのエスケープを追加しません。

## 数値の表示形式

整数は符号と十進数字です。binary 浮動小数点は同じ型へ解析し直したときに元の bit へ戻る最短の有効桁数を使います。同じ桁数なら近い十進数、等距離なら偶数の十進仮数を選びます。

| 値 | 表示 |
| --- | --- |
| `0.1`, `0.1f32` | `0.1` |
| `1.0` | `1` |
| 正負のゼロ | `0`, `-0` |
| 無限大 | `inf`, `-inf` |
| NaN | `nan` |
| `0.10d128` | `0.1` |

decimal の末尾の quantum ゼロ、NaN の符号・payload は表示・解析の往復では保持しません。

正規化した十進指数が -6 未満、または有効桁数 + 6 以上なら指数表記です。指数は小文字 e、必須の符号、先頭ゼロなしで表します。`1000000.0` は `1000000`、`10000000.0` は `1e+7` です。

## 書式指定

[補間](../language-reference/strings-and-characters.md#補間)の穴は、`{式:指定}` の形で書式を持てます。指定のない穴は `Display.display` と同じ文字列です。指定が扱うのは桁揃え、符号、精度、基数で、locale にも printf にも依存しません。数値は正確な値から実行時に整形するので、native と WASM で同じ文字列になります。

### 指定の文法

```text
指定     = [[埋め文字]揃え][+][幅][.精度][型]
埋め文字 = { } " \ と改行を除く Unicode スカラー 1 個
揃え     = < | > | ^
幅       = 1 以上 4096 以下の十進数
精度     = . と、0 以上 4096 以下の十進数
型       = x | X | o | b | e | f
```

- 埋め文字は揃えの直前にだけ置けます。省略すると空白です。
- 幅と精度の先頭に 0 は置けません。ゼロ埋めは埋め文字で `{x:0>4}` と書き、`0` フラグはありません。
- `e` と `f` には精度が必要で、`.2e`、`.2f` と書きます。精度だけの `.2` は `.2f` と同じです。`x`、`X`、`o`、`b` に精度は付けられません。
- 穴の式の最上位にある `:` が指定の始まりです。丸括弧、角括弧、波括弧の内側の `:` は始まりではありません。

幅は、表示文字列の Unicode スカラーの数で数えます。サロゲートペアは 1、孤立サロゲートも 1 で、書記素クラスターの数ではありません。表示文字列が幅に満たなければ埋め文字で埋め、幅以上ならそのままです。揃えを省くと、数値は右、それ以外（string、bool、char、`Display` の結果）は左に揃えます。`^` は不足分の半分（切り捨て）を前に、残りを後ろに置きます。

### 型ごとの規則

組み込みの型の指定は、次の規則で検査します。

| 指定 | 使える型 | それ以外 |
| --- | --- | --- |
| 埋め文字、揃え、幅 | すべての型 | なし |
| `+` | 整数、浮動小数点、decimal | `E1003` |
| 精度（`e`、`f` を含む） | 浮動小数点、decimal | `E1003` |
| `x`、`X`、`o`、`b` | 整数 | `E1003` |

`+`、精度、型文字を含む指定は、穴の値の型がその場で具体的な数値型と分かる必要があります。型変数のままだと `E1003` です。record と union の指定は、後述の `Format` クラスが受け持ちます。

### 数値の書式

- 整数の `x` は小文字、`X` は大文字の 16 進、`o` は 8 進、`b` は 2 進です。負数は `-` と絶対値で、`0x` のような接頭辞は付かず、0 は `0` です。`i128` の最小値も正しく出力します。
- `+` は 0 以上の値の前に `+` を付けます（`+0`、`+inf` を含む）。`-0.0` は `-0`（精度付きなら `-0.00`）で、NaN は常に符号のない `nan` です。
- `.Pf`（または `.P`）は小数点以下 P 桁です。P が 0 なら小数点も出しません。
- `.Pe` は有効数字 P + 1 桁の `d.ddd` に、`e` と指数を続けます。指数は負のときだけ `-` を付け、先頭に 0 を置きません（Display の `1e+7` のような `+` は付きません）。0 は `0.00e0` で、丸めで桁が上がると指数が増えます。
- 丸めは、値の正確な十進展開に対して最近接・偶数への丸めを一回だけ行います。二重丸めも、ホストの printf も使いません。
- `inf`、`-inf`、`nan` は、精度に関係なくこの表記です。
- decimal は、係数と指数が表す十進値を同じ規則で丸めます。

### 結果の例

結果は string の値を引用符付きで示します。

| 式 | 結果 |
| --- | --- |
| `$"{42:x}"` | `"2a"` |
| `$"{-255:X}"` | `"-FF"` |
| `$"{5:b}"` | `"101"` |
| `$"{8:o}"` | `"10"` |
| `$"{1:+}"` | `"+1"` |
| `$"{7:>4}"` | `"   7"` |
| `$"{7:0>4}"` | `"0007"` |
| `$"{7:<4}"` | `"7   "` |
| `$"{7:^4}"` | `" 7  "` |
| `$"{"ab":*^5}"` | `"*ab**"` |
| `$"{"ab":6}"` | `"ab    "` |
| `$"{"é𠮷":*>5}"` | `"***é𠮷"` |
| `$"{0.125:.2}"` | `"0.12"` |
| `$"{0.375:.2}"` | `"0.38"` |
| `$"{2.5:.0}"` | `"2"` |
| `$"{3.5:.0}"` | `"4"` |
| `$"{1234.5:.2e}"` | `"1.23e3"` |
| `$"{9.96:.1e}"` | `"1.0e1"` |
| `$"{0.1:.20}"` | `"0.10000000000000000555"` |
| `$"{0.1f32:.20}"` | `"0.10000000149011611938"` |
| `$"{-0.0:.2}"` | `"-0.00"` |
| `$"{1.5:+.1}"` | `"+1.5"` |

次の実行例は、上の結果の一部を出力します。`[` と `]` で囲むのは、空白の数が見えるようにするためです。

```tsuzuri run=%5B2a%5D%20%5B-FF%5D%20%5B101%5D%20%5B10%5D%20%5B%2B1%5D%0A%5B%20%20%207%5D%20%5B0007%5D%20%5B7%20%20%20%5D%20%5B%207%20%20%5D%20%5B*ab**%5D%0A%5B%20%20%207%5D%20%5Bab%20%20%5D%0A%5B0.12%5D%20%5B0.38%5D%20%5B2%5D%20%5B4%5D%0A%5B1.23e3%5D%20%5B1.0e1%5D%20%5B0.00e0%5D%0A%5B0.10000000000000000555%5D%20%5B-0.00%5D%20%5B-0%5D%20%5B%2B1.5%5D
let integers = $"[{42:x}] [{-255:X}] [{5:b}] [{8:o}] [{1:+}]"
let padding = $"[{7:>4}] [{7:0>4}] [{7:<4}] [{7:^4}] [{"ab":*^5}]"
let defaults = $"[{7:4}] [{"ab":4}]"
let rounding = $"[{0.125:.2}] [{0.375:.2}] [{2.5:.0}] [{3.5:.0}]"
let exponent = $"[{1234.5:.2e}] [{9.96:.1e}] [{0.0:.2e}]"
let exact = $"[{0.1:.20}] [{-0.0:.2}] [{-0.0:+}] [{1.5:+.1}]"
integers + "\n" + padding + "\n" + defaults + "\n" + rounding + "\n" + exponent + "\n" + exact
```

### 拒否される指定

| コード | 条件 | 例 |
| --- | --- | --- |
| `E0001` | 文法に合わない指定 | `{7:z}`、`{7:#x}` |
| `E0001` | 幅と精度が 4096 を超える、または先頭が 0 | `{7:08}`、`{7:4097}` |
| `E0001` | `x`、`X`、`o`、`b` に精度が付く | `{7:.2x}` |
| `E0001` | 精度のない `e`、`f` | `{7:e}` |
| `E1003` | 指定が値の型に合わない | `{"s":x}`、`{"s":+}`、`{7:.2}`、`{1.5:x}` |
| `E1003` | 数値の指定を付けた値の型が型変数のまま | ジェネリック関数の `{value:.2}` |

メッセージは次の形です。

```text
error[E0001]: invalid format spec 'z'; expected [[fill]align][+][width][.precision][type]
error[E0001]: format width and precision are at most 4096 and have no leading zeros; write '0>' to pad with zeros
error[E0001]: precision does not apply to integer format 'x'; remove '.2'
error[E0001]: format type 'e' needs a precision such as '.2e'
error[E1003]: format spec 'x' does not apply to string; 'x', 'X', 'o' and 'b' need an integer
error[E1003]: format spec '.2' needs a concrete numeric type, found 'a; annotate the value's type
```

### 提供しないもの

`0` フラグ（`{x:0>4}` で埋めます）、`#` による基数の接頭辞、locale と桁区切り、書記素クラスター単位の幅（Unicode 対応の D09 を待ちます）、実行時に組み立てた書式文字列、`deriving (Format)` による自動導出は提供しません。printf 風の `%` 書式文字列もありません。

## Format クラス（利用者型の書式指定）

record と union には組み込みの書式指定がありません。型クラス `Format<'a>` を実装すると、その型の穴 `{値:指定}` は、指定をインスタンスへ渡します。

| API | 所有権 |
| --- | --- |
| `Format.format value spec` | `Format<T> => ref T -> ref string -> string`。値と指定を借用し、整形した string を返す |

### 穴がインスタンスを呼ぶ条件

record または union の値に指定を付けた穴は、次のどちらかのとき `Format` インスタンスの `format` を呼びます。

- その型に `Format` インスタンスがある。
- 指定に符号、精度、型文字のどれかがある。インスタンスがなければ `E1005`（`no instance for Format<Main.Point>; define an instance or use a supported type`）です。

どちらでもない場合、つまり `Display` だけを持つ型に幅と揃えだけの指定を付けた穴は、コンパイラが `Display` の文字列を幅へ埋めます。指定のない穴は常に `Display` を使い、`Format` を呼びません。string、数値、bool、char の穴は組み込みの処理のままで、これらの型に書いた `Format` インスタンスは穴からは使われません。`u8$"..."` の穴でも規則は同じで、結果は UTF-8 へ変換されます。`instance Format<'a> => Format<Box<'a>>` のような条件付きインスタンスも使えます。

### インスタンスが受け取るもの

インスタンスは、借用した値と、検証済みの指定を正規の綴りにした string（`ref string`）を受け取ります。正規の綴りでは、既定の埋め文字（空白）と省かれた部分を書きません。`{x:*>+8.2f}` は `*>+8.2f`、`{x: <4}` は `<4`、`{x:.2}` は `.2` です。幅、揃え、埋め文字を含む整形は、すべてインスタンスの仕事です。コンパイラは戻り値を埋めも加工もしません。

```tsuzuri run=L%5Bx%5D%20D%5B%3E3%5D%20L%5B*%5E%2B8.2f%5D%20D%5B%3C4%5D%20L%5B.2%5D
union Shade = Light | Dark

instance Format<Shade> {
    fn format shade spec =
        match shade with
        | Light -> $"L[{spec}]"
        | Dark -> $"D[{spec}]"
}

$"{Light:x} {Dark:>3} {Light:*^+8.2f} {Dark: <4} {Light:.2}"
```

### Format モジュール

標準モジュール `Format` は、インスタンスが指定を扱うための型と関数を持ちます。case は `Format.AlignAuto` のように修飾して書きます。

| 名前 | 内容 |
| --- | --- |
| `Format.Align` | `Format.AlignAuto \| Format.AlignLeft \| Format.AlignCenter \| Format.AlignRight`。指定に揃えがなければ `AlignAuto` |
| `Format.Kind` | `Format.KindPlain \| Format.KindLowerHex \| Format.KindUpperHex \| Format.KindOctal \| Format.KindBinary \| Format.KindExponent \| Format.KindFixed`。型文字がなければ `KindPlain` |
| `Format.Spec` | `{ fill: string, align: Format.Align, plus: bool, width: i64, precision: i64, kind: Format.Kind }`。`fill` は Unicode スカラー 1 個で既定は空白、`width` は幅がなければ 0、`precision` はなければ -1 |
| `Format.parse text` | `ref string -> Option.Option<Format.Spec>`。指定を分解する。文法に合わない文字列、先頭が 0 の幅、4096 を超える値は `None` |
| `Format.pad spec text` | `ref Format.Spec -> string -> string`。`text` を `spec.width` 個の Unicode スカラーまで `spec.fill` で埋める。`AlignAuto` は左揃えで、中央揃えは不足分の半分（切り捨て）を前に置く。すでに幅以上なら `text` をそのまま返す |

`Format.parse` は文法だけを調べ、型文字と精度の組み合わせは検査しません。穴の指定はコンパイラが検査済みですが、`Format.format` に自分で渡した文字列は検査されません。

```tsuzuri run=fill%3D*%20plus%3Dtrue%20width%3D8%20precision%3D2%20%5B*******x%5D
let text = "*>+8.2f"
match Format.parse (ref text) with
| Option.Some spec -> $"fill={spec.fill} plus={spec.plus} width={spec.width} precision={spec.precision} [{Format.pad (ref spec) "x"}]"
| Option.None -> "invalid"
```

### Point の例

次の `Point` のインスタンスは、`+` を座標ごとの `{point.x:+}` に写し、幅と揃えを `Format.pad` に任せます。揃えを省くと `Auto` なので、`{p:12}` は左揃えです。インスタンスの中でも穴の指定は書いた通りの固定文字列で、実行時に組み立てることはできません。

```tsuzuri run=%5B%20%20%20%20(3%2C%20-4)%5D%0A%5B(3%2C%20-4)%20%20%20%20%5D%0A%5B%20%20(3%2C%20-4)%20%20%5D%0A%5B***(3%2C%20-4)%5D%0A%5B(%2B3%2C%20-4)%5D%0A%5B(3%2C%20-4)%20%20%20%20%20%5D
record Point { x: i64, y: i64 }

instance Format<Point> {
    fn format point spec =
        match Format.parse spec with
        | Option.Some parsed ->
            let text = if parsed.plus then $"({point.x:+}, {point.y:+})" else $"({point.x}, {point.y})"
            Format.pad (ref parsed) text
        | Option.None -> "invalid"
}

let p = Point { x: 3, y: -4 }
$"[{p:>11}]\n[{p:<11}]\n[{p:^11}]\n[{p:*>10}]\n[{p:+}]\n[{p:12}]"
```

`Point` は `Display` を持たないので、指定のない `{p}` は `E1005`（`no instance for Display<Main.Point>`）です。

### Display だけを持つ型

`Format` インスタンスがなく、指定が幅と揃えだけなら、コンパイラが `Display` の文字列を埋めます。

```tsuzuri run=%5BTagged%20%7B%20id%3A%207%20%7D%5D%20%5B%20%20%20%20Tagged%20%7B%20id%3A%207%20%7D%5D%20%5BTagged%20%7B%20id%3A%207%20%7D****%5D
record Tagged { id: i64 } deriving (Display)

let tag = Tagged { id: 7 }
$"[{tag}] [{tag:>20}] [{tag:*<20}]"
```

### ジェネリックコード

型変数のままの値に符号、精度、型文字を付けた穴は `E1003` です。ジェネリックな関数では、`Format.format` を直接呼びます。

```tsuzuri run=D%5B%3E3%5D
union Shade = Light | Dark

instance Format<Shade> {
    fn format shade spec =
        match shade with
        | Light -> $"L[{spec}]"
        | Dark -> $"D[{spec}]"
}

def show :: Format<'a> => ref 'a -> string
fn show value =
    let spec = ">3"
    Format.format value (ref spec)

let shade = Dark
show (ref shade)
```

### 予約名

`Format` は、型クラスの名前としても標準モジュールの名前としても予約されています。`Format` という名前の record や union は `E1001`（`duplicate or reserved name 'Format'`）、`Format.tz` というファイルは `E1011`（`module name 'Format' is reserved for the standard library; rename the file`）です。

## 解析文法

| 目標型 | 受理する入力 |
| --- | --- |
| 整数 | 任意の符号、十進数字、小文字 prefix の `0x` / `0b` |
| 浮動小数点 | 任意の符号、十進の小数・指数。`1.`, `.5`, `1e3`, `1E-3` も可 |
| 浮動小数点の特殊値 | 小文字 `inf`, `nan` と任意の符号 |
| bool | 厳密に `true` / `false` |
| char | ちょうど一 UTF-16 コード単位 |
| utf8char | ちょうど一 Unicode スカラー |

数値の `_` は桁と桁の間だけです。前後空白、空文字、途中までの解析、大文字の `0X` / `0B`、`INF` / `NaN`、hex float は認めません。

符号なし整数の負号は `-0` でも失敗です。範囲外整数、有限入力から無限大になる浮動小数点 overflow は None です。subnormal と符号付きゼロへの underflow は成功します。

数値解析は 4,096 UTF-16 コード単位まで、ASCII 数字・記号のみです。文字解析の Unicode 規則とは別です。解析失敗は診断やトラップではなく None になります。

## 独自型

```tsuzuri run=42
record Count { value: i64 }

instance Display<Count> {
    fn display count = to_string count.value
}

instance Parse<Count> {
    fn parse text =
        let number: Option<i64> = Parse.parse text
        match number with
        | Option.Some value -> Option.Some (Count { value: value })
        | Option.None -> Option.None
}

let text = "42"
let parsed: Option<Count> = Parse.parse ref text
to_string (Option.get parsed)
```

インスタンスの選択はコンパイル時です。組み込みのインスタンスは上書きできません。Display の自動導出は使えますが、任意の型の Parse 自動導出はありません。

## コンソールと制限

コンソール用ホストは同じ表示内容を UTF-8 で出力し、末尾に改行を付けます。ただし unit のコンソール結果は無出力です。単体 Display の unit は `()` です。

孤立サロゲートを持つ string / char は内部で保持できますが、UTF-8 出力時にはトラップします。明示的な修復が必要なら String.to_well_formed を使います。

printf 風の `%` 書式文字列、locale 書式、桁区切り、実行時に組み立てた書式文字列、Parse の unit / string インスタンスは提供しません。補間と書式指定は[書式指定](#書式指定)にあります。

## 関連項目

- [文字列 API](text.md)
- [文字列の補間](../language-reference/strings-and-characters.md#補間)
- [Display の自動導出](../language-reference/deriving.md)
- [型クラス](../language-reference/generics-and-typeclasses.md)
