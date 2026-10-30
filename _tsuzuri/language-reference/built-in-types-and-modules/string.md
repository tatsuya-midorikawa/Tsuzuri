# String

`String` は、UTF-16 の `string` を検索・切り出し・連結する標準モジュールです。長さの単位はコード単位で、`.length` と同じです。文字境界や書記素では切りません。

型そのものの所有権、比較、サロゲートは [文字列](../literals-and-strings/strings.md) を参照してください。ここは関数の契約です。

## この記事のポイント

- 引数が `ref string` の関数は共有借用です。入力は消費せず、戻り値は新しい所有文字列です。
- `find` と `split` は、探したい側が第 1 引数です。本文が先ではありません。
- 見つからない、範囲が不正、という失敗は `None` です。範囲外の添字と負の `repeat` はトラップします。
- 検索は素朴な走査で、最悪計算量は本文の長さとパターンの長さの積です。SIMD は保証しません。パターンで探すときは [Regex](regex.md) です。
- `clone_string` だけは `String.clone` ではありません。

> [!WARNING]
> `String.find "zu" title` です。`String.find title "zu"` ではありません。`split`、`starts_with`、`replace` も、パターンが本文より前です。

## 長さと走査

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `length` | `ref string -> i64` | コード単位数。`.length` と同じで O(1)。コピーしない |
| `char_count` | `ref string -> i64` | `length` と同じ値。スカラー数ではない。O(1) |
| `chars` | `ref string -> [char]` | 各コード単位を `char` にした新しい配列。O(n) |
| `decode_at` | `i64 -> ref string -> (char * i64)` | その位置の `char` と、次の位置。常に 1 進む |

`decode_at` はサロゲートペアを 1 文字にまとめません。添字が範囲外だとトラップします。`None` にはなりません。

```tsuzuri run=decoded%3D%5BA%5D1%20chars%3D%5B%27%5CuD83D%27%2C%20%27%5CuDE00%27%5D%20units%3D2
let text = "AB"
let decoded = match String.decode_at 0 (ref text) with
    | (value, next) -> $"[{value}]{next}"
let chars = String.chars "😀"
$"decoded={decoded} chars={chars} units={String.char_count "😀"}"
```

実行結果:

```text
decoded=[A]1 chars=['\uD83D', '\uDE00'] units=2
```

`😀` は 2 コード単位なので、`chars` も 2 要素です。配列の表示は文字を引用符で囲み、サロゲートは `\uXXXX` と出します。

`for unit in text` が返すのも `char` ではなく `i16u` です。文字が欲しいときは `Char.of_u16 unit` にします。`unit` は文字列の中の場所ではなく反復ごとの値なので、`ref unit` を反復のあとまで持ち越すことはできません（`E1013`。[for ... in](../loops-and-conditionals/for-in.md#所有権と借用ルール)）。

## 検索と比較

すべて共有借用です。空のパターンは、先頭や末尾に一致したとみなします。`find "" text` は `Some 0`、`rfind "" text` は `Some text.length`、`contains` / `starts_with` / `ends_with` の空パターンは真です。

| 関数 | シグネチャ | 失敗 | 計算量 |
| --- | --- | --- | --- |
| `find` | `ref string -> ref string -> Maybe<i64>` | 無ければ `None`。第 1 引数がパターン | 最悪 O(本文 × パターン) |
| `rfind` | `ref string -> ref string -> Maybe<i64>` | 無ければ `None`。最後の一致 | 最悪 O(本文 × パターン) |
| `contains` | `ref string -> ref string -> bool` | トラップしない | `find` と同じ |
| `starts_with` | `ref string -> ref string -> bool` | トラップしない | O(パターン) |
| `ends_with` | `ref string -> ref string -> bool` | トラップしない | O(パターン) |
| `compare` | `ref string -> ref string -> i64` | トラップしない。左が小さければ `-1`、等しければ `0`、大きければ `1` | O(短いほう) |

比較の順序は演算子 `<` と同じ、符号なし 16 bit のコード単位順です。ロケール順ではありません。

### UTF 処理の実装

ネイティブでは等値・順序比較と UTF の区間処理に 128 bit の SIMD を使います。
順序比較の短い列（UTF-16 は 16 単位未満、UTF-8 は 32 bytes 未満）は、
元の 64 bit／コード単位比較を使います。16 bytes 未満の UTF-8 復号は、
CPU dispatch やベクトル窓の検査より前に元のスカラー経路を使います。
この短い復号でも、検証後の ASCII コピーは 8 bytes 単位の一括拡幅を使います。
移植可能な 128 bit 版は、UTF-8 の ASCII と同じ幅の 2・3・4 バイトのスカラーが並ぶ区間をベクトルで復号します。
CPU ランタイム版は ASCII と同じ幅の 2・3 バイトの区間を一括で復号します。
変換・計数・UTF-8 の検証は、処理できる先頭の区間を一括で通過し、
最初に一括処理できない区間または短い末尾へ来たとき、残り全体を従来と同じスカラー処理へ一度だけ渡します。
成功した 3 バイトの区間などは、検証済みの長さだけを消費します。不均一な UTF-8 の区間でも、境界と内容が妥当なら計数・検証は一括処理できます。
スカラーへ移った後は、一括処理の検査を繰り返しません。変換は入力全体を検証・計数してから結果を確保します。
`is_well_formed` と `to_well_formed` は、先頭からサロゲートを含まない区間を一括で通過します。
最初にサロゲートを含む区間へ来たとき、または短い末尾へ来たときは、残り全体を従来と同じスカラー処理へ一度だけ渡します。
その後は SIMD の検査を繰り返しません。正しい対は保持し、孤立サロゲートだけを拒否または修復します。
`is_well_formed` と UTF-8 の検証だけを行う経路は確保をしません。UTF-8 の検証だけでは変換先の長さ上限を適用しません。
UTF-16 からの変換・計数の ASCII／同じ幅の BMP 区間処理は、判定・修復とは別の経路です。
不正な UTF-8 は結果の確保前にトラップし、修復は孤立サロゲート 1 単位ごとに U+FFFD 1 単位に置換します。

通常のネイティブビルドの長い UTF-8 復号・検証では CPU ランタイムが能力と OS の状態を検査し、AVX2 または移植可能な版を選びます。
UTF-16 符号化は AArch64 では移植可能な LLVM の計数・書き込みを使います。
x86 では長い入力の計数を CPU ランタイムが選択し、検証済みの書き込みには LLVM の経路を使います。
LLVM の符号化は初窓にサロゲートがあれば入力全体を元のスカラー経路へ一度だけ渡します。
16 単位未満の LLVM 符号化は元のスカラー検証・計数・書き込みを使い、
検証済み ASCII のコピーだけは 8 単位ずつ一括で縮幅します。
長い BMP 入力の計数は ASCII／2／3 バイト幅が混在していても一括で行います。
書き込みは同じ幅の区間だけを一括処理し、最初の非対応区間からスカラーへ戻ります。
AVX2 の実行時選択は対応する x86 のネイティブ環境で使います。Windows は現在の能力検出では移植可能な版を使います。
WASM は既定でスカラーです。`build --target wasm32 --wasm-feature simd128` の明示指定時だけ
128 bit 版を使い、新しい import は不要です。検索 API の SIMD 化、すべての入力の高速化は保証しません。

`TSUZURI_STRINGS=compact`（[コンパクト文字列の旗](../compiler/option.md#コンパクト文字列の旗)）では、Latin-1 の文字列のための kernel が加わり、選び方は上と同じです
（native は常に 128 bit 版、WASM は `simd128` のときだけ）。Latin-1 の走査・UTF-16 との混在の等値と順序比較・広げる（Latin-1 を UTF-16 の単位にする）・狭める・
Latin-1 から UTF-8 への変換・`to_ascii` の複写が対象です。`String.from_utf8` は、UTF-8 を ASCII と「`0xC2`／`0xC3` に継続バイトが続く対」として 16 bytes ずつ分類する 1 回の走査で、
全 scalar が U+00FF 以下かを調べ、そうなら 1 回の確保で Latin-1 の文字列にします（`é` を含む文章も 1 byte になります）。それ以外の入力と不正な入力は、上の UTF-16 の経路へ渡すので、
トラップの種類と条件は変わりません。非 ASCII の Latin-1 の書き込みは、ASCII だけの 16 bytes を一括で複写し、そうでない 16 bytes はスカラーで復号します。

## 切り出し

| 関数 | シグネチャ | 失敗 | 計算量 |
| --- | --- | --- | --- |
| `slice` | `i64 -> i64 -> ref string -> Maybe<string>` | 負の位置、`last < first`、範囲外なら `None` | O(切り出し長) |
| `sub` | `i64 -> i64 -> ref string -> Maybe<string>` | 負の位置、加算オーバーフロー、範囲外なら `None` | O(切り出し長) |

`slice first last text` の `last` は結果に含まれません。`sub first count text` は `first` から `count` コード単位を切り出します。どちらも新しい所有文字列を `Some` で返します。

負の位置、`last < first`、範囲外の指定、および加算オーバーフローが発生した場合は安全に `None` を返します（トラップしません）。サロゲートペアの途中インデックスであっても、位置として有効であればそのまま切り出します。

どちらも結果の長さに比例したバッファを確保してコピーします。元の文字列は借用するだけです。

## 組み立て

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `concat` | `ref [string] -> string` | 区切りなしでつなぐ。空配列は `""` |
| `join` | `ref string -> ref [string] -> string` | 区切りは第 1 引数。空配列は `""` |
| `split` | `ref string -> ref string -> [string]` | 区切りは第 1 引数。本文は第 2 引数 |
| `replace` | `ref string -> ref string -> ref string -> string` | パターン、置換、本文の順 |
| `repeat` | `i64 -> ref string -> string` | `count` 回繰り返す。0 回は `""` |

`split` の区切りが空なら、コード単位のあいだで切ります。`"ab"` は `["a", "b"]`、空文字列は空配列です。区切りが空でないときは、一致ごとに切ります。先頭、末尾、連続した一致は空要素になります。空文字列を `","` で割ると `[""]` です。両端に、一致以外の空要素は足しません。

`replace` は左から、重ならない一致を置換します。置換後の文字列は再検索しません。空のパターンは、コード単位の境界と両端に挿入します。空文字列には 1 回だけ挿入します。

`repeat` の負の回数はトラップします。結果が 2^53 − 1 コード単位を超えるとき、または長さの積が溢れるときもトラップします。`join`、`concat`、`replace` も、結果の長さを先に検査してから 1 回だけ確保します。

```tsuzuri run=index%3D3%20parts%3D%5B%22a%22%2C%20%22%22%2C%20%22b%22%5D%20piece%3DTsu%20lower%3Dtsuzuri
let title = "Tsuzuri"
let found = String.find "zu" title
let parts = String.split "," "a,,b"
let piece = String.slice 0 3 title
let index = match found with
    | Some value -> to_string value
    | None -> "none"
let text = match piece with
    | Some value -> value
    | None -> "none"
$"index={index} parts={parts} piece={text} lower={String.to_ascii_lower title}"
```

実行結果:

```text
index=3 parts=["a", "", "b"] piece=Tsu lower=tsuzuri
```

配列の表示は要素の文字列を引用符で囲みます。`parts` の 2 番目は空文字列です。

```tsuzuri run=dashed%3D-a-b-%20once%3D-%20rep%3Dhahaha
let dashed = String.replace "" "-" "ab"
let once = String.replace "" "-" ""
$"dashed={dashed} once={once} rep={String.repeat 3 "ha"}"
```

実行結果:

```text
dashed=-a-b- once=- rep=hahaha
```

## 空白と ASCII の大小

| 関数 | シグネチャ | 説明 | 計算量 |
| --- | --- | --- | --- |
| `trim` | `ref string -> string` | 先頭と末尾の ASCII 空白を除去 | O(n) |
| `trim_start` | `ref string -> string` | 先頭の ASCII 空白を除去 | O(n) |
| `trim_end` | `ref string -> string` | 末尾の ASCII 空白を除去 | O(n) |
| `to_ascii_lower` | `ref string -> string` | ASCII 英大文字（`A`〜`Z`）を小文字に変換 | O(n) |
| `to_ascii_upper` | `ref string -> string` | ASCII 英小文字（`a`〜`z`）を大文字に変換 | O(n) |

`trim`、`trim_start`、`trim_end` が取り除くのは ASCII の空白文字だけです。具体的には、コード単位 9〜13（タブ、改行、垂直タブ、フォームフィード、復帰）と 32（空白）が対象です。全角空白は除去しません。新しい所有文字列を返し、失敗することはありません。

`to_ascii_lower` と `to_ascii_upper` は、ASCII の英文字だけを大文字・小文字に変換します。それ以外のコード単位は変更せずそのまま保持します。すべての文字の大文字小文字の変換は [Unicode](unicode.md) の `to_lower`・`to_upper` です。

## 所有権を移す変換

次の関数は入力を消費します。ヒープ上のバッファなら、ポインタを移すだけでコピーしません。定数領域のリテラルは、先にヒープへコピーされます。エンコーディングは変えません。

| 関数 | シグネチャ | 失敗 |
| --- | --- | --- |
| `to_code_units` | `string -> [i16u]` | しない。孤立サロゲートも残る |
| `from_code_units` | `[i16u] -> string` | 長さが 2^53 − 1 を超えるとトラップ |
| `clone_string` | `ref string -> string` | しない。O(n) のコピー。入力は残る |

`clone_string` はモジュール名が付きません。`Utf8String.clone` と対になる組み込み関数です。

> [!NOTE]
> 「ポインタを移すだけ」は、既定の `TSUZURI_STRINGS=utf16` の契約です。`compact`（opt-in。[コンパクト文字列の旗](../compiler/option.md#コンパクト文字列の旗)）では、
> Latin-1 で持っている文字列の `to_code_units` は、バッファを `realloc` で 2 倍にして 1 byte の列をその場で 2 byte の単位へ広げ（伸ばせなければ新しいバッファへ移る）、`from_code_units` は全単位が 0xFF 以下なら 1 byte へその場で狭めて `realloc` でバッファを縮めるので、どちらも O(n) です。
> 結果の値、孤立サロゲートが残ること、失敗の条件は変わりません。UTF-16 の文字列では上のとおり、コピーなしです。

## エンコーディング

| 関数 | シグネチャ | 失敗 | 計算量 |
| --- | --- | --- | --- |
| `from_utf8` | `ref utf8string -> string` | 入力は常に妥当な UTF-8。上限超過はトラップ | O(n) |
| `is_well_formed` | `ref string -> bool` | しない。孤立サロゲートが無ければ真 | O(n) |
| `to_well_formed` | `ref string -> string` | しない。孤立サロゲートを U+FFFD にする | O(n) |

`Utf8String.from_string` は逆向きで、孤立サロゲートではトラップします。コンソール出力も UTF-8 なので、整形式でない `string` をそのまま最後の式にするとトラップします。先に `is_well_formed` を見るか、`to_well_formed` を明示してください。

## 計算量

計算量は 1 回の呼び出しの最悪の時間です。`n` は `text`（対象の文字列）のコード単位数、`m` は `needle`・`separator`・`prefix`・`suffix` のコード単位数、`k` は結果のコード単位数です（`join`・`concat` の `n` は部品の数）。規則は [構造化タグ](../organizing-tsuzuri/documentation-comment.md#構造化タグ) にあります。表は `std/String.tz` の `@complexity` から `node scripts/gen-docs.mjs` が作ります。組み込み関数のうち `compare`・`from_utf8`・`is_well_formed`・`to_well_formed` の計算量は、各節の表の `計算量` の列にあります。

<!-- generated:complexity:String -->
| 関数 | 計算量 |
| --- | --- |
| `String.length` | $O(1)$ |
| `String.find` | $O(n \cdot m)$ |
| `String.rfind` | $O(n \cdot m)$ |
| `String.contains` | $O(n \cdot m)$ |
| `String.starts_with` | $O(m)$ |
| `String.ends_with` | $O(m)$ |
| `String.slice` | $O(k)$ |
| `String.sub` | $O(k)$ |
| `String.decode_at` | $O(1)$ |
| `String.char_count` | $O(1)$ |
| `String.chars` | $O(n)$ |
| `String.join` | $O(n + k)$ |
| `String.concat` | $O(n + k)$ |
| `String.split` | $O(n \cdot m)$ |
| `String.trim_start` | $O(n)$ |
| `String.trim_end` | $O(n)$ |
| `String.trim` | $O(n)$ |
| `String.to_ascii_lower` | $O(n)$ |
| `String.to_ascii_upper` | $O(n)$ |
| `String.repeat` | $O(k)$ |
| `String.replace` | $O(n \cdot m + k)$ |
<!-- /generated:complexity:String -->

## 注意点

> [!NOTE]
> 関数はカリー化されています。`String.find "zu"` は、残りの `string` を受け取る関数です。`"Tsuzuri" |> String.find "zu"` のようにパイプラインへ渡せます。

> [!WARNING]
> `slice` と `sub` の失敗は `None`、`decode_at` と添字の範囲外はトラップです。同じ「範囲外」でも戻り方が違います。

### 引数の順序

標準ライブラリ全体の規則は、[標準ライブラリの引数の順序](./argument-order.md)にまとめてあります。

読み取る文字列は、必ず最後の引数です。`"Tsuzuri" |> String.sub 1 3` のようにパイプで渡せ、`String.sub 1 3 >> Maybe.default_value ""` のように、文字列だけが足りない関数を合成できます。位置や個数などそれ以外の引数は、関数名が示す順で文字列の前に並べます（`String.slice first last text`、`String.sub first count text`、`String.decode_at offset text`、`String.repeat count text`）。パターンや区切りを取る `find`・`split`・`replace` も、本文が最後です。`repeat` の順は、F# の `String.replicate count str` と同じです。

```tsuzuri run=word%3Dsuz%20loud%3DSUZ%20twice%3Dsuzsuz
let middle = String.sub 1 3 >> Maybe.default_value ""
let word = "Tsuzuri" |> middle
let loud = word |> String.to_ascii_upper
let twice = word |> String.repeat 2
$"word={word} loud={loud} twice={twice}"
```

実行結果:

```text
word=suz loud=SUZ twice=suzsuz
```

文字列を借用する関数に `word |> String.repeat 2` と渡しても、`word` は消費されません。

## まとめ

- 読む関数は `ref` で、戻り値は新しい所有文字列です。
- パターンを取る関数は、パターンが第 1 引数です。
- 切り出しの失敗は `None`、添字と負の繰り返しはトラップです。
- 空の区切りはコード単位の境界、空でない区切りは空要素を残します。
- 孤立サロゲートを UTF-8 へ出す前に、検査か置換を明示します。

## 関連項目

- [Unicode](unicode.md) — 正規化、書記素クラスター、単語境界、完全な大文字小文字の変換
- [Regex](regex.md) — 正規表現による検索・置換・分割
- [文字列](../literals-and-strings/strings.md)
- [Utf8String](utf8string.md)
- [Char](char.md)
- [Format](format.md)
- [言語リファレンスの目次](../index.md)
