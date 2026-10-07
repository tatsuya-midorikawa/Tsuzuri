# Unicode

`Unicode` は、Unicode 文字データベース（UCD）17.0.0 から作った表を引く標準モジュールです。[Regex](./regex.md) の `\p{...}`、`\d`・`\w`・`\s` と大文字小文字を区別しない照合は、このモジュールと同じ表を使います。

表は `scripts/generate-unicode.mjs` が UCD から生成したランタイムの定数（`src/runtime/unicode.ll`）で、`Unicode` か `Regex` の関数を使うプログラムにだけ連結されます。使わないプログラムの生成コードと WebAssembly の import は変わりません。

## この記事のポイント

- 版は `Unicode.version ()` で `"17.0.0"` です。版を上げると結果が変わるので、版は固定しています。
- `Unicode.property_ranges name` は、`\p{name}` が一致するスカラーの閉区間を昇順に返します。未知の名前は `None` です。
- `Unicode.simple_case_folding ()` は、単純な大文字小文字の畳み込みで別の文字に移るスカラー `c` と移り先の組 `(c, scf c)` を `c` の昇順に返します。
- どちらも呼び出しごとに表全体を読み、新しい配列を確保します。繰り返し使うときは結果を保持してください。

## 版と property の表

`property_ranges` が受け付ける名前は `Regex` の `\p{...}` と同じです。一般カテゴリーの短い名前 30 個（`Lu`、`Ll`、`Lt`、`Lm`、`Lo`、`Mn`、`Mc`、`Me`、`Nd`、`Nl`、`No`、`Pc`、`Pd`、`Ps`、`Pe`、`Pi`、`Pf`、`Po`、`Sm`、`Sc`、`Sk`、`So`、`Zs`、`Zl`、`Zp`、`Cc`、`Cf`、`Cs`、`Co`、`Cn`）、群 `L`、`LC`、`M`、`N`、`P`、`S`、`Z`、`C`、二値 property の `Alphabetic`、`White_Space`、`Join_Control` です。長い名前（`Uppercase_Letter`）、`gc=Lu`、`Script=Latin` は `None` です。

```tsuzuri run=17.0.0%20digits%3D770%20first%3D%2848%2C%2057%29%20unknown%3Dtrue
let version = Unicode.version ()
let name = "Nd"
let digits = Maybe.get (Unicode.property_ranges (ref name))
let total = Array.fold (\count range -> match range with | (low, high) -> count + (high - low + 1)) 0 (ref digits)
let first = digits[0]
let unknown_name = "Greek"
let unknown = Maybe.is_none (ref (Unicode.property_ranges (ref unknown_name)))
$"{version} digits={total} first={first} unknown={unknown}"
```

実行結果:

```text
17.0.0 digits=770 first=(48, 57) unknown=true
```

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

## API リファレンス

すべての関数は `std::Unicode` モジュールに属しています。

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `version` | `unit -> string` | 表の Unicode の版 `"17.0.0"` |
| `property_ranges` | `ref string -> Maybe<[(i64 * i64)]>` | `\p{name}` のスカラーの閉区間（昇順・連結済み）。未知の名前は `None` |
| `simple_case_folding` | `unit -> [(i64 * i64)]` | `(c, scf c)` の組（`c != scf c`、`c` の昇順） |

## 計算量

| 操作 | 計算量 | 備考 |
| --- | --- | --- |
| `version` | $O(1)$ | 文字列を 1 つ確保します |
| `property_ranges` | $O(\text{表の項目数})$ | 一般カテゴリーは 4,144 個の連続区間、`Alphabetic` は 761 区間を読みます |
| `simple_case_folding` | $O(\text{組の数})$ | 209 個の連続した組から 1,512 組を復元します |

## まとめ

- 表は UCD 17.0.0 から生成し、使うプログラムにだけ連結されます。
- `property_ranges` と `simple_case_folding` は `Regex` と同じ表を返すので、`\p{...}` や `i` の意味を確かめられます。
- 呼び出しごとに表を読み直して確保するので、結果は保持して使います。

## 関連項目

- [Regex](./regex.md) — `\p{...}`、`\w`、大文字小文字を区別しない照合
- [Utf8Char](./utf8char.md) — Unicode スカラー
- [String](./string.md)
- [言語リファレンスの目次](../index.md)
