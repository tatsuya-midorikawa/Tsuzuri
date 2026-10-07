# Unicode

`Unicode` は、Unicode 文字データベース（UCD）17.0.0 から作った表を引く標準モジュールです。一般カテゴリー、正規化（NFC・NFD・NFKC・NFKD）、書記素クラスターと単語の境界（UAX #29）、完全な大文字小文字の変換を、`string`（UTF-16）と `utf8string`（UTF-8）の両方に提供します。[Regex](./regex.md) の `\p{...}`、`\d`・`\w`・`\s` と大文字小文字を区別しない照合も、同じ表を使います。

表は `scripts/generate-unicode.mjs` が UCD から生成したランタイムの定数（`src/runtime/unicode.ll`）で、`Unicode` か `Regex` の関数を使うプログラムにだけ連結されます。使わないプログラムの生成コードと WebAssembly の import は変わりません。

## この記事のポイント

- 版は `Unicode.version ()` で `"17.0.0"` です。版を上げると結果が変わるので、版は固定しています。
- `Unicode.category scalar` は `utf8char` の一般カテゴリーを union `Unicode.Category`（`Lu` … `Cn`）で返します。
- `Unicode.normalize form text` は `Unicode.Nfc`・`Nfd`・`Nfkc`・`Nfkd` の形に正規化した新しい文字列を返します。`is_normalized` はその形かどうかです。
- `graphemes` と `grapheme_boundaries` は拡張書記素クラスター（利用者が 1 文字と見る単位）、`words` と `word_boundaries` は既定の単語境界です。どちらも UAX #29 の規則で、ロケールによる調整や辞書は使いません。
- `to_lower`・`to_upper`・`to_title`・`case_fold` は `SpecialCasing.txt` を含む完全な変換です（`ß` は `SS`）。ロケールによる調整（トルコ語の `i` など）はしません。
- `utf8string` の関数は名前に `_utf8` が付きます。境界の位置は `string` ではコード単位、`utf8string` では byte です。`string` の孤立サロゲートは、変換されずにそのまま残ります。
- `property_ranges` と `simple_case_folding` は表全体を読んで配列を返します。繰り返し使うときは結果を保持してください。

## 版と property の表

`property_ranges` が受け付ける名前は `Regex` の `\p{...}` と同じです。一般カテゴリーの短い名前 30 個（`Lu`、`Ll`、`Lt`、`Lm`、`Lo`、`Mn`、`Mc`、`Me`、`Nd`、`Nl`、`No`、`Pc`、`Pd`、`Ps`、`Pe`、`Pi`、`Pf`、`Po`、`Sm`、`Sc`、`Sk`、`So`、`Zs`、`Zl`、`Zp`、`Cc`、`Cf`、`Cs`、`Co`、`Cn`）、群 `L`、`LC`、`M`、`N`、`P`、`S`、`Z`、`C`、二値 property の `Alphabetic`、`White_Space`、`Join_Control` です。長い名前（`Uppercase_Letter`）、`gc=Lu`、`Script=Latin` は `None` です。

```tsuzuri run=17.0.0%20ranges%3D72%20first%3D%2848%2C%2057%29%20last%3D%28130032%2C%20130041%29%20unknown%3Dtrue
let version = Unicode.version ()
let name = "Nd"
let digits = Maybe.get (Unicode.property_ranges (ref name))
let count = digits.length
let first = digits[0]
let last = digits[count - 1]
let unknown_name = "Greek"
let unknown = Maybe.is_none (ref (Unicode.property_ranges (ref unknown_name)))
$"{version} ranges={count} first={first} last={last} unknown={unknown}"
```

実行結果:

```text
17.0.0 ranges=72 first=(48, 57) last=(130032, 130041) unknown=true
```

`Nd`（10 進数字）は 72 区間で、最初は ASCII の `0`〜`9`（48〜57）、最後は U+1FBF0〜U+1FBF9 です。
区間は隙間なく連結済みで、`Cs`（サロゲート）は U+D800〜U+DFFF、`Cn`（未割り当て）は非文字を含みます。

## 単純な大文字小文字の畳み込み

`simple_case_folding` は `CaseFolding.txt` の状態 C（共通）と S（単純）の対応です。長さが変わる状態 F（完全）とトルコ語の状態 T は含みません。`scf (scf c) == scf c` が成り立ち、同じ文字へ畳み込まれる文字どうしが `Regex` の `i` フラグで一致します。

```tsuzuri run=pairs%3D1512%20first%3D%2865%2C%2097%29%20last%3D%28125217%2C%20125251%29
let pairs = Unicode.simple_case_folding ()
let count = pairs.length
let first = pairs[0]
let last = pairs[count - 1]
$"pairs={count} first={first} last={last}"
```

実行結果:

```text
pairs=1512 first=(65, 97) last=(125217, 125251)
```

最初の組は `A`（65）から `a`（97）です。ケルビン記号（U+212A = 8490）は `k`（107）へ畳み込まれます。

## 一般カテゴリー

`Unicode.category` は 1 スカラーの一般カテゴリーを返します。union `Unicode.Category` のケース名は UCD の短い名前（`Lu`、`Ll`、`Lt`、`Lm`、`Lo`、`Mn`、`Mc`、`Me`、`Nd`、`Nl`、`No`、`Pc`、`Pd`、`Ps`、`Pe`、`Pi`、`Pf`、`Po`、`Sm`、`Sc`、`Sk`、`So`、`Zs`、`Zl`、`Zp`、`Cc`、`Cf`、`Cs`、`Co`、`Cn`）で、`Eq` と `Display` を持ちます。`utf8char` はサロゲートを持たないので、`Cs` は返りません。

```tsuzuri run=Lu%20Nd%20So
let a = Unicode.category u8'A'
let b = Unicode.category u8'٣'
let c = Unicode.category u8'😀'
$"{a} {b} {c}"
```

実行結果:

```text
Lu Nd So
```

## 正規化

`normalize` は UAX #15 の正規化です。`Nfd` は正準分解（canonical decomposition）、`Nfkd` は互換分解、`Nfc` と `Nfkc` は分解の後で正準合成をします。分解は再帰的に行い、ハングル音節は算術で分解・合成します。結合文字の並びは正準結合クラスの順に安定に並べ替え、合成では `Full_Composition_Exclusion` の文字を作りません。

```tsuzuri run=nfd%3D2%20nfc%3D1%20nfkc%3Dfi1%20same%3Dtrue
let composed = "é"
let decomposed = Unicode.normalize Unicode.Nfd (ref composed)
let again = Unicode.normalize Unicode.Nfc (ref decomposed)
let compatible = Unicode.normalize Unicode.Nfkc (ref "ﬁ①")
let same = Unicode.is_normalized Unicode.Nfc (ref again)
let nfd = decomposed.length
let nfc = again.length
$"nfd={nfd} nfc={nfc} nfkc={compatible} same={same}"
```

実行結果:

```text
nfd=2 nfc=1 nfkc=fi1 same=true
```

`is_normalized form text` は、`normalize form text` が `text` と等しいかどうかです。文字列の比較（`==`）は正規化しないので、見た目が同じ文字列を等しいとみなしたいときは、両方を同じ形に正規化してから比べます。

## 書記素クラスター

`graphemes` は拡張書記素クラスター（UAX #29 の GB1〜GB999。インド系文字の結合 GB9c と絵文字の ZWJ 列 GB11 を含む）の列を返します。`grapheme_boundaries` はその境界の位置で、先頭の 0 と末尾の長さを含みます（空の文字列は `[0]`）。境界の数から 1 を引くとクラスターの数です。

```tsuzuri run=clusters%3D3%20offsets%3D%5B0%2C%202%2C%2010%2C%2014%5D%20units%3D14
let text = "e\u{301}👨‍👩‍👧🇯🇵"
let clusters = Unicode.graphemes (ref text)
let count = clusters.length
let offsets = Unicode.grapheme_boundaries (ref text)
let units = text.length
$"clusters={count} offsets={offsets} units={units}"
```

実行結果:

```text
clusters=3 offsets=[0, 2, 10, 14] units=14
```

`e` と結合アクセント、家族の絵文字（4 個の ZWJ 結合）、国旗（地域指示子 2 個）がそれぞれ 1 クラスターです。

## 単語境界

`words` は UAX #29 の既定の単語境界（WB1〜WB999）で切った部分文字列の列で、単語だけでなく空白や句読点の区切りも含みます。`word_boundaries` はその境界の位置です。`It's` や `3.14` は 1 つの単語です。

```tsuzuri run=%5B%22It%27s%22%2C%20%22%20%22%2C%20%223.14%22%2C%20%22%20%22%2C%20%22meters%22%2C%20%22.%22%5D
let words = Unicode.words (ref "It's 3.14 meters.")
$"{words}"
```

実行結果:

```text
["It's", " ", "3.14", " ", "meters", "."]
```

## 大文字小文字の変換

`to_lower`・`to_upper` は `UnicodeData.txt` の単純な対応と `SpecialCasing.txt` の条件なしの対応による完全な変換で、長さが変わることがあります。`to_lower` は語末の `Σ` を `ς` にします（Final_Sigma。前に cased な文字があり、後ろに続かないとき。case-ignorable な文字は飛ばします）。
`to_title` は、各単語（単語境界の間）の最初の cased な文字をタイトルケースにし、その後をすべて小文字にします（Unicode 標準 §3.13 の toTitlecase）。`case_fold` は `CaseFolding.txt` の状態 C と F の完全な畳み込みで、大文字小文字を区別しない比較に使います。

```tsuzuri run=STRASSE%20%CE%BF%CE%B4%CE%BF%CF%82%20Hello%20World%20strasse
let upper = Unicode.to_upper (ref "straße")
let lower = Unicode.to_lower (ref "ΟΔΟΣ")
let title = Unicode.to_title (ref "hello wORLD")
let folded = Unicode.case_fold (ref "Straße")
$"{upper} {lower} {title} {folded}"
```

実行結果:

```text
STRASSE οδος Hello World strasse
```

ロケールに依存する条件付きの対応（トルコ語・アゼルバイジャン語の `I` と `i`、リトアニア語の点）は行いません。ASCII だけを変える `String.to_ascii_upper` などより遅く、結果を新しく確保します。

## API リファレンス

すべての関数は `std::Unicode` モジュールに属しています。

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `version` | `unit -> string` | 表の Unicode の版 `"17.0.0"` |
| `category` | `utf8char -> Category` | 一般カテゴリー |
| `property_ranges` | `ref string -> Maybe<[(i64 * i64)]>` | `\p{name}` のスカラーの閉区間（昇順・連結済み）。未知の名前は `None` |
| `simple_case_folding` | `unit -> [(i64 * i64)]` | `(c, scf c)` の組（`c != scf c`、`c` の昇順） |
| `normalize` / `normalize_utf8` | `NormalizationForm -> ref string -> string` | 正規化した新しい文字列 |
| `is_normalized` / `is_normalized_utf8` | `NormalizationForm -> ref string -> bool` | すでにその形か |
| `graphemes` / `graphemes_utf8` | `ref string -> [string]` | 拡張書記素クラスターの列 |
| `grapheme_boundaries` / `grapheme_boundaries_utf8` | `ref string -> [i64]` | クラスターの境界の位置（0 と長さを含む） |
| `words` / `words_utf8` | `ref string -> [string]` | 単語境界の間の部分文字列の列 |
| `word_boundaries` / `word_boundaries_utf8` | `ref string -> [i64]` | 単語境界の位置（0 と長さを含む） |
| `to_lower` / `to_lower_utf8` | `ref string -> string` | 完全な小文字化（Final_Sigma を含む） |
| `to_upper` / `to_upper_utf8` | `ref string -> string` | 完全な大文字化 |
| `to_title` / `to_title_utf8` | `ref string -> string` | 単語ごとのタイトルケース |
| `case_fold` / `case_fold_utf8` | `ref string -> string` | 完全な大文字小文字の畳み込み |

`_utf8` の関数は `string` を `utf8string` に読み替えたシグネチャです。`Category` は 30 ケースの union、`NormalizationForm` は `Nfc | Nfd | Nfkc | Nfkd` で、どちらも `Eq` と `Display` を持ちます。

## 計算量

| 操作 | 計算量 | 備考 |
| --- | --- | --- |
| `version` | $O(1)$ | 文字列を 1 つ確保します |
| `category` | $O(\log t)$ | 一般カテゴリーの 4,144 個の連続区間を二分探索します |
| `property_ranges` | $O(t)$ | 一般カテゴリーは 4,144 区間、`Alphabetic` は 761 区間を読みます |
| `simple_case_folding` | $O(t)$ | 209 個の連続した組から 1,512 組を復元します |
| `normalize`、`is_normalized` | $O(n \log t + k \log k)$ | 各スカラーの分解・結合クラス・合成を二分探索で引きます。$k$ は連続する結合文字の最長の数で、その並べ替えは整列です |
| `graphemes`、`words` と境界 | $O(n \log t)$ | 単語境界の先読み（Extend・Format・ZWJ の続きの次の文字）は、後ろからの 1 回の走査で前もって求めます |
| `to_lower` などの変換 | $O(n \log t)$ | 語末のシグマの判定は前後の case-ignorable な文字の続きを読みます。続きを読むのは両隣の `Σ` だけなので、合わせても $O(n)$ です |

$n$ は入力のスカラー数、$t$ は表の項目数です。結果と作業用の配列（スカラーの列）は呼び出しごとに確保します。文字列全体の ASCII の高速経路や SIMD はなく、速度は他言語の実装と比べていません。

## まとめ

- 表は UCD 17.0.0 から生成し、使うプログラムにだけ連結されます。
- カテゴリー、正規化、書記素クラスター、単語境界、完全な大文字小文字の変換を `string` と `utf8string` で使えます。
- 境界は UAX #29 の既定の規則で、ロケールによる調整はしません。大文字小文字の変換もロケールに依存しません。
- `property_ranges` と `simple_case_folding` は `Regex` と同じ表を返すので、`\p{...}` や `i` の意味を確かめられます。

## 関連項目

- [Regex](./regex.md) — `\p{...}`、`\w`、大文字小文字を区別しない照合
- [String](./string.md) と [Utf8String](./utf8string.md) — コード単位とバイトでの検索・切り出し
- [文字列](../literals-and-strings/strings.md) — 長さの単位と比較
- [Utf8Char](./utf8char.md) — Unicode スカラー
- [言語リファレンスの目次](../index.md)
